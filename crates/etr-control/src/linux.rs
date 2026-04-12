#![cfg(target_os = "linux")]

use aya::maps::HashMap;
use aya::programs::{SchedClassifier, TcAttachType, tc};
use aya::{Ebpf, EbpfLoader};
use etr_config::EtrConfig;
use etr_types::{
    FlowStateKey, FlowStateValue, ForwardRuleKey, ForwardRuleValue, TC_EGRESS_PROGRAM_NAME,
    TC_FLOW_STATE_MAP, TC_FORWARD_RULES_MAP, TC_INGRESS_PROGRAM_NAME,
};
use std::path::PathBuf;
use tokio::sync::Mutex;
use tracing::info;

use crate::dataplane::{
    ApplyReport, DataPlane, DataPlaneError, DataPlaneStatus, build_installed_rules,
    count_rule_changes,
};
use crate::kernel::encode_enabled_rules;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PodForwardRuleKey(ForwardRuleKey);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PodForwardRuleValue(ForwardRuleValue);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PodFlowStateKey(FlowStateKey);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PodFlowStateValue(FlowStateValue);

unsafe impl aya::Pod for PodForwardRuleKey {}
unsafe impl aya::Pod for PodForwardRuleValue {}
unsafe impl aya::Pod for PodFlowStateKey {}
unsafe impl aya::Pod for PodFlowStateValue {}

pub struct LinuxTcDataPlane {
    interface: String,
    object_path: PathBuf,
    state: Mutex<Option<LinuxTcState>>,
}

struct LinuxTcState {
    bpf: Ebpf,
    installed_rules: Vec<crate::dataplane::InstalledRule>,
}

impl LinuxTcDataPlane {
    pub fn new(interface: String, object_path: PathBuf) -> Result<Self, DataPlaneError> {
        if interface.trim().is_empty() {
            return Err(DataPlaneError::Setup(
                "external interface must not be empty".to_owned(),
            ));
        }

        Ok(Self {
            interface,
            object_path,
            state: Mutex::new(None),
        })
    }

    fn initialize_bpf(&self) -> Result<Ebpf, DataPlaneError> {
        let mut bpf = EbpfLoader::new()
            .load_file(&self.object_path)
            .map_err(|error| {
                DataPlaneError::Setup(format!(
                    "failed to load eBPF object {}: {error}",
                    self.object_path.display()
                ))
            })?;

        if let Err(error) = tc::qdisc_add_clsact(&self.interface) {
            if error.raw_os_error() != Some(17) {
                return Err(DataPlaneError::Setup(format!(
                    "failed to add clsact qdisc on {}: {error}",
                    self.interface
                )));
            }
        }

        let _ = tc::qdisc_detach_program(
            &self.interface,
            TcAttachType::Ingress,
            TC_INGRESS_PROGRAM_NAME,
        );
        let _ = tc::qdisc_detach_program(
            &self.interface,
            TcAttachType::Egress,
            TC_EGRESS_PROGRAM_NAME,
        );

        {
            let ingress: &mut SchedClassifier = bpf
                .program_mut(TC_INGRESS_PROGRAM_NAME)
                .ok_or_else(|| {
                    DataPlaneError::Setup(format!(
                        "missing ingress program '{TC_INGRESS_PROGRAM_NAME}' in eBPF object"
                    ))
                })?
                .try_into()
                .map_err(|error| {
                    DataPlaneError::Setup(format!("ingress program type mismatch: {error}"))
                })?;
            ingress.load().map_err(|error| {
                DataPlaneError::Setup(format!("failed to load ingress program: {error}"))
            })?;
            ingress
                .attach(&self.interface, TcAttachType::Ingress)
                .map_err(|error| {
                    DataPlaneError::Setup(format!("failed to attach ingress program: {error}"))
                })?;
        }

        {
            let egress: &mut SchedClassifier = bpf
                .program_mut(TC_EGRESS_PROGRAM_NAME)
                .ok_or_else(|| {
                    DataPlaneError::Setup(format!(
                        "missing egress program '{TC_EGRESS_PROGRAM_NAME}' in eBPF object"
                    ))
                })?
                .try_into()
                .map_err(|error| {
                    DataPlaneError::Setup(format!("egress program type mismatch: {error}"))
                })?;
            egress.load().map_err(|error| {
                DataPlaneError::Setup(format!("failed to load egress program: {error}"))
            })?;
            egress
                .attach(&self.interface, TcAttachType::Egress)
                .map_err(|error| {
                    DataPlaneError::Setup(format!("failed to attach egress program: {error}"))
                })?;
        }

        Ok(bpf)
    }

    fn sync_maps(&self, bpf: &mut Ebpf, config: &EtrConfig) -> Result<(), DataPlaneError> {
        let encoded_rules = encode_enabled_rules(config);

        {
            let mut rule_map: HashMap<_, PodForwardRuleKey, PodForwardRuleValue> =
                HashMap::try_from(bpf.map_mut(TC_FORWARD_RULES_MAP).ok_or_else(|| {
                    DataPlaneError::MapSync(format!(
                        "missing map '{TC_FORWARD_RULES_MAP}' in eBPF object"
                    ))
                })?)
                .map_err(|error| {
                    DataPlaneError::MapSync(format!("rule map open failed: {error}"))
                })?;

            let stale_keys = rule_map
                .keys()
                .filter_map(Result::ok)
                .collect::<Vec<PodForwardRuleKey>>();
            for key in stale_keys {
                rule_map.remove(&key).map_err(|error| {
                    DataPlaneError::MapSync(format!("failed to remove stale rule key: {error}"))
                })?;
            }

            for rule in &encoded_rules {
                rule_map
                    .insert(
                        PodForwardRuleKey(rule.key),
                        PodForwardRuleValue(rule.value),
                        0,
                    )
                    .map_err(|error| {
                        DataPlaneError::MapSync(format!(
                            "failed to insert rule '{}' into forward map: {error}",
                            rule.name
                        ))
                    })?;
            }
        }

        {
            let mut flow_map: HashMap<_, PodFlowStateKey, PodFlowStateValue> =
                HashMap::try_from(bpf.map_mut(TC_FLOW_STATE_MAP).ok_or_else(|| {
                    DataPlaneError::MapSync(format!(
                        "missing map '{TC_FLOW_STATE_MAP}' in eBPF object"
                    ))
                })?)
                .map_err(|error| {
                    DataPlaneError::MapSync(format!("flow map open failed: {error}"))
                })?;

            let stale_keys = flow_map
                .keys()
                .filter_map(Result::ok)
                .collect::<Vec<PodFlowStateKey>>();
            for key in stale_keys {
                let _ = flow_map.remove(&key);
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl DataPlane for LinuxTcDataPlane {
    async fn apply_config(&self, config: &EtrConfig) -> Result<ApplyReport, DataPlaneError> {
        let next_rules = build_installed_rules(config);
        let mut state_guard = self.state.lock().await;
        let mut state = match state_guard.take() {
            Some(existing) => existing,
            None => LinuxTcState {
                bpf: self.initialize_bpf()?,
                installed_rules: Vec::new(),
            },
        };

        self.sync_maps(&mut state.bpf, config)?;

        let changed_rules = count_rule_changes(&state.installed_rules, &next_rules);
        state.installed_rules = next_rules;

        info!(
            backend = "tc-aya",
            interface = self.interface.as_str(),
            object_path = %self.object_path.display(),
            rules = state.installed_rules.len(),
            changed_rules,
            "applied configuration to Linux TC data plane"
        );

        let applied_rules = state.installed_rules.len();
        *state_guard = Some(state);

        Ok(ApplyReport {
            backend: "tc-aya".to_owned(),
            applied_rules,
            changed_rules,
        })
    }

    async fn status(&self) -> DataPlaneStatus {
        let state = self.state.lock().await;
        let installed_rules = state
            .as_ref()
            .map(|state| state.installed_rules.len())
            .unwrap_or(0);

        DataPlaneStatus {
            backend: "tc-aya".to_owned(),
            installed_rules,
            interface: Some(self.interface.clone()),
        }
    }
}

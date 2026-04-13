#![cfg(target_os = "linux")]

use aya::maps::{HashMap, PerCpuArray};
use aya::programs::{SchedClassifier, TcAttachType, tc};
use aya::{Ebpf, EbpfLoader};
use etr_config::EtrConfig;
use etr_types::{
    FlowStateKey, FlowStateValue, ForwardRuleKey, ForwardRuleValue, RuntimeStat,
    TC_EGRESS_PROGRAM_NAME, TC_FLOW_STATE_MAP, TC_FORWARD_RULES_MAP, TC_INGRESS_PROGRAM_NAME,
    TC_RUNTIME_STATS_MAP,
};
use std::path::PathBuf;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::dataplane::{
    ApplyReport, DataPlane, DataPlaneError, DataPlaneStats, DataPlaneStatus, PreflightReport,
    build_installed_rules, count_rule_changes,
};
use crate::kernel::encode_enabled_rules;
use crate::preflight::run_linux_preflight;

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
    preflight: PreflightReport,
    state: Mutex<Option<LinuxTcState>>,
}

struct LinuxTcState {
    bpf: Ebpf,
    installed_rules: Vec<crate::dataplane::InstalledRule>,
}

impl LinuxTcDataPlane {
    pub fn new(
        interface: String,
        object_path: PathBuf,
        allow_preflight_warnings: bool,
    ) -> Result<Self, DataPlaneError> {
        if interface.trim().is_empty() {
            return Err(DataPlaneError::Setup(
                "external interface must not be empty".to_owned(),
            ));
        }

        let preflight = run_linux_preflight(&interface, &object_path, allow_preflight_warnings);
        if !preflight.passed && !allow_preflight_warnings {
            return Err(DataPlaneError::Preflight(preflight.summary()));
        }
        if !preflight.passed {
            warn!(
                interface = interface.as_str(),
                object_path = %object_path.display(),
                failures = preflight.summary(),
                "Linux TC preflight failed but override is active"
            );
        }

        Ok(Self {
            interface,
            object_path,
            preflight,
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

    fn count_flow_entries(bpf: &mut Ebpf) -> Result<usize, DataPlaneError> {
        let flow_map: HashMap<_, PodFlowStateKey, PodFlowStateValue> =
            HashMap::try_from(bpf.map_mut(TC_FLOW_STATE_MAP).ok_or_else(|| {
                DataPlaneError::MapSync(format!("missing map '{TC_FLOW_STATE_MAP}' in eBPF object"))
            })?)
            .map_err(|error| DataPlaneError::MapSync(format!("flow map open failed: {error}")))?;

        Ok(flow_map.keys().filter_map(Result::ok).count())
    }

    fn read_stats(bpf: &mut Ebpf) -> Result<DataPlaneStats, DataPlaneError> {
        let stats_map: PerCpuArray<_, u64> =
            PerCpuArray::try_from(bpf.map_mut(TC_RUNTIME_STATS_MAP).ok_or_else(|| {
                DataPlaneError::MapSync(format!(
                    "missing map '{TC_RUNTIME_STATS_MAP}' in eBPF object"
                ))
            })?)
            .map_err(|error| DataPlaneError::MapSync(format!("stats map open failed: {error}")))?;

        Ok(DataPlaneStats {
            ingress_rule_hits: sum_stat(&stats_map, RuntimeStat::IngressRuleHits)?,
            ingress_reverse_hits: sum_stat(&stats_map, RuntimeStat::IngressReverseHits)?,
            egress_flow_hits: sum_stat(&stats_map, RuntimeStat::EgressFlowHits)?,
            flow_creations: sum_stat(&stats_map, RuntimeStat::FlowCreations)?,
            rule_misses: sum_stat(&stats_map, RuntimeStat::RuleMisses)?,
            parse_drops: sum_stat(&stats_map, RuntimeStat::ParseDrops)?,
        })
    }
}

fn sum_stat(
    stats_map: &PerCpuArray<&mut aya::maps::MapData, u64>,
    stat: RuntimeStat,
) -> Result<u64, DataPlaneError> {
    stats_map
        .get(&stat.as_u32(), 0)
        .map(|values| values.iter().copied().sum())
        .map_err(|error| DataPlaneError::MapSync(format!("stats read failed: {error}")))
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
        let mut state = self.state.lock().await;
        let mut diagnostics_error = None;
        let (installed_rules, flow_entries, stats) = match state.as_mut() {
            Some(state) => {
                let installed_rules = state.installed_rules.len();
                let flow_entries = match Self::count_flow_entries(&mut state.bpf) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        diagnostics_error = Some(error.to_string());
                        None
                    }
                };
                let stats = match Self::read_stats(&mut state.bpf) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        diagnostics_error = Some(error.to_string());
                        None
                    }
                };
                (installed_rules, flow_entries, stats)
            }
            None => (0, None, None),
        };

        DataPlaneStatus {
            backend: "tc-aya".to_owned(),
            installed_rules,
            interface: Some(self.interface.clone()),
            object_path: Some(self.object_path.display().to_string()),
            flow_entries,
            stats,
            preflight: Some(self.preflight.clone()),
            diagnostics_error,
        }
    }
}

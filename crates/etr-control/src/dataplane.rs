use async_trait::async_trait;
use etr_config::{DataPlaneKind, EtrConfig, Protocol, SnatMode};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApplyReport {
    pub backend: String,
    pub applied_rules: usize,
    pub changed_rules: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DataPlaneStatus {
    pub backend: String,
    pub installed_rules: usize,
    pub interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledRule {
    name: String,
    protocol: Protocol,
    frontend: String,
    backend: String,
    snat: SnatMode,
}

#[derive(Debug, Error)]
pub enum DataPlaneError {
    #[error("unsupported protocol in TC backend: {0}")]
    UnsupportedProtocol(String),
    #[error("TC data plane object path is required on Linux")]
    MissingObjectPath,
    #[error("data plane kind '{0}' is not implemented")]
    UnsupportedDataPlaneKind(String),
    #[error("Linux TC backend is unavailable on this operating system")]
    LinuxBackendUnavailable,
    #[error("Linux TC backend setup failed: {0}")]
    Setup(String),
    #[error("Linux TC backend map sync failed: {0}")]
    MapSync(String),
}

#[async_trait]
pub trait DataPlane: Send + Sync {
    async fn apply_config(&self, config: &EtrConfig) -> Result<ApplyReport, DataPlaneError>;
    async fn status(&self) -> DataPlaneStatus;
}

#[derive(Default)]
pub struct TcDataPlane {
    installed_rules: RwLock<Vec<InstalledRule>>,
}

impl TcDataPlane {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DataPlane for TcDataPlane {
    async fn apply_config(&self, config: &EtrConfig) -> Result<ApplyReport, DataPlaneError> {
        let next_rules = build_installed_rules(config);

        let mut installed_rules = self.installed_rules.write().await;
        let previous_rules = installed_rules.clone();
        let changed_rules = count_rule_changes(&previous_rules, &next_rules);

        *installed_rules = next_rules;

        info!(
            backend = "tc-stub",
            rules = installed_rules.len(),
            changed_rules,
            "applied configuration to TC data plane placeholder"
        );

        Ok(ApplyReport {
            backend: "tc-stub".to_owned(),
            applied_rules: installed_rules.len(),
            changed_rules,
        })
    }

    async fn status(&self) -> DataPlaneStatus {
        let installed_rules = self.installed_rules.read().await;
        DataPlaneStatus {
            backend: "tc-stub".to_owned(),
            installed_rules: installed_rules.len(),
            interface: None,
        }
    }
}

pub fn build_data_plane(
    config: &EtrConfig,
    bpf_object: Option<PathBuf>,
) -> Result<Arc<dyn DataPlane>, DataPlaneError> {
    match config.data_plane.kind {
        DataPlaneKind::Tc => build_tc_data_plane(config, bpf_object),
    }
}

fn build_tc_data_plane(
    config: &EtrConfig,
    bpf_object: Option<PathBuf>,
) -> Result<Arc<dyn DataPlane>, DataPlaneError> {
    #[cfg(target_os = "linux")]
    {
        let object_path = bpf_object.ok_or(DataPlaneError::MissingObjectPath)?;
        let data_plane = crate::linux::LinuxTcDataPlane::new(
            config.data_plane.external_interface.clone(),
            object_path,
        )?;
        return Ok(Arc::new(data_plane));
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        let _ = bpf_object;
        Ok(Arc::new(TcDataPlane::new()))
    }
}

pub(crate) fn build_installed_rules(config: &EtrConfig) -> Vec<InstalledRule> {
    config
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            let backend = &rule.backends[0];
            InstalledRule {
                name: rule.name.clone(),
                protocol: rule.protocol,
                frontend: rule.frontend_label(),
                backend: format!("{}:{}", backend.addr, backend.port),
                snat: rule.snat,
            }
        })
        .collect()
}

pub(crate) fn count_rule_changes(
    previous_rules: &[InstalledRule],
    next_rules: &[InstalledRule],
) -> usize {
    next_rules
        .iter()
        .filter(|candidate| !previous_rules.contains(candidate))
        .count()
}

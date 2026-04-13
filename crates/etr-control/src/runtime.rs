use crate::dataplane::{ApplyReport, DataPlane, DataPlaneError, DataPlaneStatus};
use etr_config::{ConfigError, EtrConfig};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum ControlPlaneError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("failed to apply config to data plane: {0}")]
    DataPlane(#[from] DataPlaneError),
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub config_path: PathBuf,
    pub applied_at_unix_ms: u128,
    pub active_config: EtrConfig,
    pub last_apply_report: ApplyReport,
    pub data_plane_status: DataPlaneStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReloadOutcome {
    pub snapshot: RuntimeSnapshot,
}

#[derive(Clone)]
pub struct AppRuntime {
    config_path: PathBuf,
    data_plane: Arc<dyn DataPlane>,
    state: Arc<RwLock<RuntimeState>>,
}

#[derive(Debug, Clone)]
struct RuntimeState {
    applied_at_unix_ms: u128,
    active_config: EtrConfig,
    last_apply_report: ApplyReport,
}

impl AppRuntime {
    pub async fn bootstrap(
        config_path: impl Into<PathBuf>,
        data_plane: Arc<dyn DataPlane>,
    ) -> Result<Self, ControlPlaneError> {
        let config_path = config_path.into();
        let active_config = EtrConfig::load_from_path(&config_path)?;
        let last_apply_report = data_plane.apply_config(&active_config).await?;

        let state = RuntimeState {
            applied_at_unix_ms: now_unix_ms(),
            active_config,
            last_apply_report,
        };

        Ok(Self {
            config_path,
            data_plane,
            state: Arc::new(RwLock::new(state)),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub async fn snapshot(&self) -> RuntimeSnapshot {
        let state = self.state.read().await;
        RuntimeSnapshot {
            config_path: self.config_path.clone(),
            applied_at_unix_ms: state.applied_at_unix_ms,
            active_config: state.active_config.clone(),
            last_apply_report: state.last_apply_report.clone(),
            data_plane_status: self.data_plane.status().await,
        }
    }

    pub async fn reload(&self) -> Result<ReloadOutcome, ControlPlaneError> {
        let active_config = EtrConfig::load_from_path(&self.config_path)?;
        let last_apply_report = self.data_plane.apply_config(&active_config).await?;
        let applied_at_unix_ms = now_unix_ms();

        {
            let mut state = self.state.write().await;
            state.applied_at_unix_ms = applied_at_unix_ms;
            state.active_config = active_config;
            state.last_apply_report = last_apply_report;
        }

        Ok(ReloadOutcome {
            snapshot: self.snapshot().await,
        })
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis()
}

use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use etr_config::EtrConfig;
use etr_control::{AppRuntime, build_data_plane};
use serde::Serialize;
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "etrd", about = "etr control-plane daemon")]
struct Cli {
    #[arg(long, env = "ETR_CONFIG", default_value = "config/etr.toml")]
    config: PathBuf,
    #[arg(long, env = "ETR_BPF_OBJECT")]
    bpf_object: Option<PathBuf>,
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_filter: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_filter)?;

    let bootstrap_config = EtrConfig::load_from_path(&cli.config)
        .with_context(|| format!("failed to parse config {}", cli.config.display()))?;
    let data_plane = build_data_plane(&bootstrap_config, cli.bpf_object.clone())
        .with_context(|| "failed to build selected data plane".to_owned())?;
    let runtime = AppRuntime::bootstrap(cli.config.clone(), data_plane)
        .await
        .with_context(|| format!("failed to bootstrap etr from {}", cli.config.display()))?;

    let snapshot = runtime.snapshot().await;
    let management_addr = snapshot.active_config.management.listen;

    info!(
        node = snapshot.active_config.service.node_name.as_str(),
        config_path = %runtime.config_path().display(),
        backend = snapshot.data_plane_status.backend.as_str(),
        rules = snapshot.last_apply_report.applied_rules,
        interface = snapshot.data_plane_status.interface.as_deref().unwrap_or("n/a"),
        listen = %management_addr,
        "etr daemon bootstrapped"
    );

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/admin/reload", post(reload_config))
        .with_state(runtime);

    let listener = tokio::net::TcpListener::bind(management_addr)
        .await
        .with_context(|| format!("failed to bind management API to {management_addr}"))?;

    info!(listen = %management_addr, "management API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("management API server stopped unexpectedly")?;

    Ok(())
}

async fn healthz(State(runtime): State<AppRuntime>) -> Json<HealthResponse> {
    let snapshot = runtime.snapshot().await;

    Json(HealthResponse {
        status: "ok",
        node_name: snapshot.active_config.service.node_name,
        config_path: snapshot.config_path.display().to_string(),
        rule_count: snapshot.last_apply_report.applied_rules,
        data_plane_backend: snapshot.data_plane_status.backend,
        data_plane_interface: snapshot.data_plane_status.interface,
        last_reload_unix_ms: snapshot.applied_at_unix_ms,
    })
}

async fn get_config(State(runtime): State<AppRuntime>) -> Json<etr_control::RuntimeSnapshot> {
    Json(runtime.snapshot().await)
}

async fn reload_config(
    State(runtime): State<AppRuntime>,
) -> Result<Json<etr_control::ReloadOutcome>, ApiError> {
    runtime.reload().await.map(Json).map_err(ApiError::from)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    node_name: String,
    config_path: String,
    rule_count: usize,
    data_plane_backend: String,
    data_plane_interface: Option<String>,
    last_reload_unix_ms: u128,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl From<etr_control::ControlPlaneError> for ApiError {
    fn from(value: etr_control::ControlPlaneError) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: value.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        error!(
            status = self.status.as_u16(),
            error = self.message.as_str(),
            "request failed"
        );
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

fn init_tracing(filter: &str) -> anyhow::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter.to_owned()));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .try_init()
        .map_err(|error| anyhow::anyhow!("failed to initialize tracing subscriber: {error}"))
}

async fn shutdown_signal() {
    if let Err(error) = signal::ctrl_c().await {
        error!(error = %error, "failed to listen for shutdown signal");
    }
}

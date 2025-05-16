use std::net::SocketAddr;

use anyhow::Context as _;
use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::future;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    cluster_types::{Cluster, DeploymentName, NamespaceName},
    k8s::{Scaler, Watcher},
};

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

pub async fn run_server(
    port: u16,
    watcher: Watcher,
    scaler: Scaler,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::debug!("Starting Autoscaler agent on {bind_address}");
    let app = create_agent_router(watcher, scaler);

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding Autoscaler agent to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop request sender for Autoscaler agent was dropped without sending a signal"
                );
            }
            tracing::info!("Stop request received, Autoscaler agent is shutting down");
        })
        .await
        .context("Autoscaler agent failed")?;
    tracing::info!("Autoscaler agent shut down");
    Ok(())
}

fn create_agent_router(watcher: Watcher, scaler: Scaler) -> Router {
    let app = App { watcher, scaler };
    Router::new()
        .route("/healthz", get(health))
        .route("/cluster", get(get_cluster))
        .route("/scale", post(scale))
        .with_state(app)
}

// TODO: Use
// https://github.com/matter-labs/zksync-era/blob/9821a20018c367ce246dba656daab5c2e7757973/core/node/api_server/src/healthcheck.rs#L53
// instead.
async fn health() -> &'static str {
    "Ok\n"
}

#[derive(Clone)]
struct App {
    watcher: Watcher,
    scaler: Scaler,
}

async fn get_cluster(State(app): State<App>) -> Result<Json<Cluster>, AppError> {
    let cluster = app.watcher.cluster.lock().await.clone();
    Ok(Json(cluster))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScaleDeploymentRequest {
    pub namespace: NamespaceName,
    pub name: DeploymentName,
    pub size: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScaleRequest {
    pub deployments: Vec<ScaleDeploymentRequest>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScaleResponse {
    pub scale_result: Vec<String>,
}

/// To test or forse scale in particular cluster use:
/// $ curl -X POST -H "Content-Type: application/json" --data '{"deployments": [{"namespace": "prover-red", "name": "witness-vector-generator-spec-9-f", "size":0},{"namespace": "prover-red", "name": "witness-vector-generator-spec-9-c", "size":0}]}' <ip>:8081/scale
async fn scale(
    State(app): State<App>,
    Json(payload): Json<ScaleRequest>,
) -> Result<Json<ScaleResponse>, AppError> {
    let handles: Vec<_> = payload
        .deployments
        .into_iter()
        .map(|d| {
            let s = app.scaler.clone();
            tokio::spawn(async move {
                match s.scale(&d.namespace, &d.name, d.size).await {
                    Ok(()) => "".to_string(),
                    Err(err) => err.to_string(),
                }
            })
        })
        .collect();

    let scale_result = future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    Ok(Json(ScaleResponse { scale_result }))
}

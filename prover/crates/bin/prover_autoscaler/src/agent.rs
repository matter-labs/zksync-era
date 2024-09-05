use std::net::SocketAddr;

use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use reqwest::StatusCode;
use tokio::sync::watch;

use crate::k8s::{Cluster, Watcher};

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
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_address = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::debug!("Starting proof data handler server on {bind_address}");
    let app = create_agent_router(watcher);

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding proof data handler server to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop signal sender for proof data handler server was dropped without sending a signal");
            }
            tracing::info!("Stop signal received, proof data handler server is shutting down");
        })
        .await
        .context("Proof data handler server failed")?;
    tracing::info!("Proof data handler server shut down");
    Ok(())
}

async fn health() -> &'static str {
    "Ok\n"
}

fn create_agent_router(watcher: Watcher) -> Router {
    let app = App { watcher };
    Router::new()
        .route("/healthz", get(health))
        .route("/cluster", get(get_cluster))
        .with_state(app)
}

#[derive(Clone)]
struct App {
    watcher: Watcher,
}

async fn get_cluster(State(app): State<App>) -> Result<Json<Cluster>, AppError> {
    let cluster = app.watcher.cluster.lock().await.clone();
    Ok(Json(cluster))
}

mod common;
mod disperser;

use std::net::SocketAddr;
use anyhow::Context as _;
use axum::{
    routing::{get, put},
    Router,
};
use tokio::sync::watch;

pub async fn run_server(mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
    // TODO: Replace port for config
    let bind_address = SocketAddr::from(([0, 0, 0, 0], 4242));
    tracing::debug!("Starting eigenda proxy on {bind_address}");
    let app = create_eigenda_proxy_router();

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding eigenda proxy to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for eigenda proxy was dropped without sending a signal"
                );
            }
            tracing::info!("Stop signal received, eigenda proxy is shutting down");
        })
        .await
        .context("EigenDA proxy failed")?;
    tracing::info!("EigenDA proxy shut down");
    Ok(())
}

fn create_eigenda_proxy_router() -> Router {
    let router = Router::new()
        .route(
            "/get/",
            get(|| async { todo!("Handle eigenda proxy get request") }),
        )
        .route(
            "/put/",
            put(|| async { todo!("Handle eigenda proxy put request") }),
        );
    router
}

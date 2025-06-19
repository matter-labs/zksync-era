use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use tokio::sync::watch;
use zksync_health_check::{AppHealth, AppHealthCheck};

async fn check_health(
    app_health_check: State<Arc<AppHealthCheck>>,
) -> (StatusCode, Json<AppHealth>) {
    let response = app_health_check.check_health().await;
    let response_code = if response.is_healthy() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (response_code, Json(response))
}

async fn run_server_inner(
    bind_address: &SocketAddr,
    app_health_check: Arc<AppHealthCheck>,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    tracing::debug!(
        "Starting healthcheck server with checks {app_health_check:?} on {bind_address}"
    );

    app_health_check.expose_metrics();
    let app = Router::new()
        .route("/health", get(check_health))
        .with_state(app_health_check);
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding healthcheck server to {bind_address}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!("Stop request sender for healthcheck server was dropped without sending a request");
            }
            tracing::info!("Stop request received, healthcheck server is shutting down");
        })
        .await
        .context("Healthcheck server failed")?;
    tracing::info!("Healthcheck server shut down");
    Ok(())
}

pub(crate) async fn run_server(
    addr: SocketAddr,
    app_health_check: Arc<AppHealthCheck>,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // Paradoxically, `hyper` server is quite slow to shut down if it isn't queried during shutdown:
    // <https://github.com/hyperium/hyper/issues/3188>. It is thus recommended to set a timeout for shutdown.
    const GRACEFUL_SHUTDOWN_WAIT: Duration = Duration::from_secs(10);

    let server_future = run_server_inner(&addr, app_health_check, stop_receiver.clone());
    tokio::pin!(server_future);

    tokio::select! {
        server_result = &mut server_future => {
            server_result?;
            anyhow::ensure!(*stop_receiver.borrow(), "Healthcheck server stopped on its own");
            Ok(())
        }
        _ = stop_receiver.changed() => {
            let server_result = tokio::time::timeout(GRACEFUL_SHUTDOWN_WAIT, server_future).await;
            if let Ok(server_result) = server_result {
                // Propagate potential errors from the server task.
                server_result
            } else {
                tracing::debug!("Timed out {GRACEFUL_SHUTDOWN_WAIT:?} waiting for healthcheck server to gracefully shut down");
                Ok(())
            }
        }
    }
}

use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use tokio::sync::watch;
use zksync_config::configs::api;
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

async fn run_server(
    bind_address: &api::BindAddress,
    app_health_check: Arc<AppHealthCheck>,
    mut stop_receiver: watch::Receiver<bool>,
) {
    tracing::info!(
        ?bind_address,
        "Starting healthcheck server with checks {app_health_check:?}"
    );

    app_health_check.expose_metrics();
    let app = Router::new()
        .route("/health", get(check_health))
        .with_state(app_health_check);
    let graceful_shutdown = async move {
        if stop_receiver.changed().await.is_err() {
            tracing::warn!(
                "Stop request sender for healthcheck server was dropped without sending a request"
            );
        }
        tracing::info!("Stop request received, healthcheck server is shutting down");
    };

    match bind_address {
        api::BindAddress::Tcp(addr) => {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .unwrap_or_else(|err| panic!("Failed binding healthcheck server to {addr}: {err}"));
            axum::serve(listener, app)
                .with_graceful_shutdown(graceful_shutdown)
                .await
                .expect("Healthcheck server failed");
        }
        #[cfg(unix)]
        api::BindAddress::Unix(path) => {
            let listener = tokio::net::UnixListener::bind(path).unwrap_or_else(|err| {
                panic!(
                    "Failed binding healthcheck server to {path}: {err}",
                    path = path.display()
                )
            });
            axum::serve(listener, app)
                .with_graceful_shutdown(graceful_shutdown)
                .await
                .expect("Healthcheck server failed");
        }
    }
    tracing::info!("Healthcheck server shut down");

    #[cfg(unix)]
    if let api::BindAddress::Unix(path) = bind_address {
        tracing::info!(path = %path.display(), "Removing Unix domain socket");
        if let Err(err) = tokio::fs::remove_file(path).await {
            tracing::error!(path = %path.display(), %err, "Failed removing Unix domain socket");
        }
    }
}

#[derive(Debug)]
pub struct HealthCheckHandle {
    server: tokio::task::JoinHandle<()>,
    stop_sender: watch::Sender<bool>,
}

impl HealthCheckHandle {
    pub fn spawn_server(addr: api::BindAddress, app_health_check: Arc<AppHealthCheck>) -> Self {
        let (stop_sender, stop_receiver) = watch::channel(false);
        let server = tokio::spawn(async move {
            run_server(&addr, app_health_check, stop_receiver).await;
        });

        Self {
            server,
            stop_sender,
        }
    }

    pub async fn stop(self) {
        // Paradoxically, `hyper` server is quite slow to shut down if it isn't queried during shutdown:
        // <https://github.com/hyperium/hyper/issues/3188>. It is thus recommended to set a timeout for shutdown.
        const GRACEFUL_SHUTDOWN_WAIT: Duration = Duration::from_secs(10);

        self.stop_sender.send(true).ok();
        let server_result = tokio::time::timeout(GRACEFUL_SHUTDOWN_WAIT, self.server).await;
        if let Ok(server_result) = server_result {
            // Propagate potential panics from the server task.
            server_result.unwrap();
        } else {
            tracing::debug!("Timed out {GRACEFUL_SHUTDOWN_WAIT:?} waiting for healthcheck server to gracefully shut down");
        }
    }
}

use std::{
    future::{Future, IntoFuture},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use futures::FutureExt;
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
    local_addr_sender: watch::Sender<Option<api::BindAddress>>,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
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

    let (server_future, local_addr) = match bind_address {
        api::BindAddress::Tcp(addr) => {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .with_context(|| format!("failed binding healthcheck server to {addr}"))?;
            let local_addr = listener
                .local_addr()
                .context("failed getting local address")?;
            let server_future = axum::serve(listener, app)
                .with_graceful_shutdown(graceful_shutdown)
                .into_future()
                .left_future();
            (server_future, api::BindAddress::Tcp(local_addr))
        }
        #[cfg(unix)]
        api::BindAddress::Unix(path) => {
            let listener = tokio::net::UnixListener::bind(path).with_context(|| {
                format!(
                    "failed binding healthcheck server to domain socket {}",
                    path.display()
                )
            })?;
            let canonical_path = tokio::fs::canonicalize(path).await.with_context(|| {
                format!(
                    "failed getting canonical domain socket path for {}",
                    path.display()
                )
            })?;

            let server_future = axum::serve(listener, app)
                .with_graceful_shutdown(graceful_shutdown)
                .into_future()
                .right_future();
            (server_future, api::BindAddress::Unix(canonical_path))
        }
    };
    local_addr_sender.send_replace(Some(local_addr.clone()));
    tracing::info!(?local_addr, "Started healthcheck server");
    server_future.await?;
    tracing::info!("Healthcheck server shut down");

    #[cfg(unix)]
    if let api::BindAddress::Unix(path) = &local_addr {
        tracing::info!(path = %path.display(), "Removing Unix domain socket");
        if let Err(err) = tokio::fs::remove_file(path).await {
            tracing::error!(path = %path.display(), %err, "Failed removing Unix domain socket");
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct HealthCheckHandle {
    server: tokio::task::JoinHandle<()>,
    stop_sender: watch::Sender<bool>,
    local_addr: watch::Receiver<Option<api::BindAddress>>,
}

impl HealthCheckHandle {
    pub fn spawn_server(addr: api::BindAddress, app_health_check: Arc<AppHealthCheck>) -> Self {
        let (stop_sender, stop_receiver) = watch::channel(false);
        let (local_addr_sender, local_addr) = watch::channel(None);
        let server = tokio::spawn(async move {
            // FIXME: doesn't really work; will be fixed after https://github.com/matter-labs/zksync-era/pull/4213 is merged.
            run_server(&addr, app_health_check, local_addr_sender, stop_receiver)
                .await
                .unwrap();
        });

        Self {
            server,
            stop_sender,
            local_addr,
        }
    }

    /// Returns the local address the server is bound to.
    pub fn local_addr(&self) -> impl Future<Output = Option<api::BindAddress>> {
        let mut local_addr = self.local_addr.clone();
        // `unwrap()` is safe by construction: after the address is set, it's never updated
        async move {
            Some(
                local_addr
                    .wait_for(Option::is_some)
                    .await
                    .ok()?
                    .clone()
                    .unwrap(),
            )
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

#[cfg(test)]
mod tests {
    use std::{fmt, io, path::PathBuf, pin::Pin, task::Poll};

    use http::{Response, Uri};
    use http_body_util::BodyExt;
    use hyper_util::{
        client::legacy::Client,
        rt::{TokioExecutor, TokioIo},
    };
    use tokio::net::UnixStream;
    use zksync_health_check::{HealthStatus, ReactiveHealthCheck};

    use super::*;

    fn mock_health() -> Arc<AppHealthCheck> {
        let health = AppHealthCheck::new(None, None);
        health.set_details(serde_json::json!({ "version": "0.1.0" }));
        let (check, health_updater) = ReactiveHealthCheck::new("test");
        health_updater.update(HealthStatus::Ready.into());
        health_updater.freeze();
        health.insert_component(check).unwrap();
        Arc::new(health)
    }

    #[tokio::test]
    async fn http_server() {
        let server = HealthCheckHandle::spawn_server(0.into(), mock_health());
        let local_addr = server.local_addr().await.expect("server has not started");
        let local_addr = local_addr.as_tcp().unwrap();

        let client = Client::builder(TokioExecutor::new()).build_http::<String>();
        let uri = format!("http://{local_addr}/health").parse().unwrap();
        let response = client.get(uri).await.unwrap();
        assert_response(response).await;
    }

    async fn assert_response(response: Response<impl BodyExt<Error: fmt::Debug>>) {
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = &response.headers()[http::header::CONTENT_TYPE];
        assert_eq!(content_type, "application/json");

        let response_body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
        assert_eq!(json["details"], serde_json::json!({ "version": "0.1.0" }));
        assert_eq!(json["components"]["test"]["status"], "ready");
    }

    /// `hyper`-compatible connector for domain sockets.
    #[derive(Debug, Clone)]
    struct UdsConnector(PathBuf);

    impl tower::Service<Uri> for UdsConnector {
        type Response = TokioIo<UnixStream>;
        type Error = io::Error;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Uri) -> Self::Future {
            let path = self.0.clone();
            Box::pin(async { UnixStream::connect(path).await.map(TokioIo::new) })
        }
    }

    #[tokio::test]
    async fn uds_server() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let bind_to = api::BindAddress::Unix(temp_dir.path().join("health.sock"));
        let server = HealthCheckHandle::spawn_server(bind_to, mock_health());
        let local_addr = server.local_addr().await.expect("server has not started");
        let api::BindAddress::Unix(path) = local_addr else {
            panic!("Unexpected local address: {local_addr:?}");
        };

        let client = Client::builder(TokioExecutor::new()).build::<_, String>(UdsConnector(path));
        let uri = "http://test/health".parse().unwrap();
        let response = client.get(uri).await.unwrap();
        assert_response(response).await;
    }
}

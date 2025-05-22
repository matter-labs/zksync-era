use std::sync::Arc;

use serde::Serialize;
use zksync_config::configs::api::HealthCheckConfig;
use zksync_health_check::{node::AppHealthCheckResource, AppHealthCheck};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_metrics::metadata::{GitMetadata, RustMetadata, GIT_METRICS, RUST_METRICS};

use crate::healthcheck::HealthCheckHandle;

/// Full metadata of the compiled binary.
#[derive(Debug, Serialize)]
pub struct BinMetadata {
    pub rust: &'static RustMetadata,
    pub git: &'static GitMetadata,
}

/// Wiring layer for health check server
///
/// Expects other layers to insert different components' health checks
/// into [`AppHealthCheck`] aggregating heath using [`AppHealthCheckResource`].
/// The added task spawns a health check server that only exposes the state provided by other tasks.
#[derive(Debug)]
pub struct HealthCheckLayer(pub HealthCheckConfig);

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    pub app_health_check: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub health_check_task: HealthCheckTask,
}

#[async_trait::async_trait]
impl WiringLayer for HealthCheckLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "healthcheck_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let AppHealthCheckResource(app_health_check) = input.app_health_check;
        app_health_check.override_limits(self.0.slow_time_limit, self.0.hard_time_limit);

        let health_check_task = HealthCheckTask {
            config: self.0,
            app_health_check,
        };

        Ok(Output { health_check_task })
    }
}

#[derive(Debug)]
pub struct HealthCheckTask {
    config: HealthCheckConfig,
    app_health_check: Arc<AppHealthCheck>,
}

#[async_trait::async_trait]
impl Task for HealthCheckTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "healthcheck_server".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.app_health_check.set_details(BinMetadata {
            rust: RUST_METRICS.initialize(),
            git: GIT_METRICS.initialize(),
        });
        let handle =
            HealthCheckHandle::spawn_server(self.config.bind_addr(), self.app_health_check);
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

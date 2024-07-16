use std::sync::Arc;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_health_check::AppHealthCheck;
use zksync_node_api_server::healthcheck::HealthCheckHandle;

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for health check server
///
/// Expects other layers to insert different components' health checks
/// into [`AppHealthCheck`] aggregating heath using [`AppHealthCheckResource`].
/// The added task spawns a health check server that only exposes the state provided by other tasks.
#[derive(Debug)]
pub struct HealthCheckLayer(pub HealthCheckConfig);

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    #[context(default)]
    pub app_health_check: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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
        app_health_check.override_limits(self.0.slow_time_limit(), self.0.hard_time_limit());

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
        let handle =
            HealthCheckHandle::spawn_server(self.config.bind_addr(), self.app_health_check.clone());
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

use std::sync::Arc;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_health_check::AppHealthCheck;
use zksync_node_api_server::healthcheck::HealthCheckHandle;

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for health check server
///
/// Expects other layers to insert different components' health checks
/// into [`AppHealthCheck`] aggregating heath using [`AppHealthCheckResource`].
/// The added task spawns a health check server that only exposes the state provided by other tasks.
///
/// ## Requests resources
///
/// - `AppHealthCheckResource`
///
/// ## Adds tasks
///
/// - `HealthCheckTask`
#[derive(Debug)]
pub struct HealthCheckLayer(pub HealthCheckConfig);

#[async_trait::async_trait]
impl WiringLayer for HealthCheckLayer {
    fn layer_name(&self) -> &'static str {
        "healthcheck_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        let AppHealthCheckResource(app_health_check) = node.get_resource_or_default();

        let task = HealthCheckTask {
            config: self.0,
            app_health_check,
        };

        node.add_task(task);
        Ok(())
    }
}

#[derive(Debug)]
struct HealthCheckTask {
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

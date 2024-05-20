use std::sync::Arc;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_health_check::AppHealthCheck;
use zksync_node_api_server::healthcheck::HealthCheckHandle;

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::{ServiceContext, StopReceiver},
    task::UnconstrainedTask,
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for a health check server.
///
/// Spawned task collects all the health checks added by different tasks to the
/// corresponding resource collection and spawns an HTTP server exposing them.
///
/// This layer expects other tasks to add health checks to the `ResourceCollection<HealthCheckResource>`.
///
/// ## Effects
///
/// - Resolves `ResourceCollection<HealthCheckResource>`.
/// - Adds `healthcheck_server` to the node.
#[derive(Debug)]
pub struct HealthCheckLayer(pub HealthCheckConfig);

#[async_trait::async_trait]
impl WiringLayer for HealthCheckLayer {
    fn layer_name(&self) -> &'static str {
        "healthcheck_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        let AppHealthCheckResource(app_health_check) = node.get_resource_or_default().await;

        let task = HealthCheckTask {
            config: self.0,
            app_health_check,
        };

        // Healthcheck server only exposes the state provided by other tasks, and also it has to start as soon as possible.
        node.add_unconstrained_task(Box::new(task));
        Ok(())
    }
}

#[derive(Debug)]
struct HealthCheckTask {
    config: HealthCheckConfig,
    app_health_check: Arc<AppHealthCheck>,
}

#[async_trait::async_trait]
impl UnconstrainedTask for HealthCheckTask {
    fn name(&self) -> &'static str {
        "healthcheck_server"
    }

    async fn run_unconstrained(
        mut self: Box<Self>,
        mut stop_receiver: StopReceiver,
    ) -> anyhow::Result<()> {
        let handle =
            HealthCheckHandle::spawn_server(self.config.bind_addr(), self.app_health_check.clone());
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

use std::fmt;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_core::api_server::healthcheck::HealthCheckHandle;

use crate::{
    implementations::resources::healthcheck::HealthCheckResource,
    resource::ResourceCollection,
    service::{ServiceContext, StopReceiver},
    task::Task,
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
        let healthchecks = node
            .get_resource_or_default::<ResourceCollection<HealthCheckResource>>()
            .await;

        let task = HealthCheckTask {
            config: self.0,
            healthchecks,
        };

        node.add_task(Box::new(task));
        Ok(())
    }
}

struct HealthCheckTask {
    config: HealthCheckConfig,
    healthchecks: ResourceCollection<HealthCheckResource>,
}

impl fmt::Debug for HealthCheckTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthCheckTask")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Task for HealthCheckTask {
    fn name(&self) -> &'static str {
        "healthcheck_server"
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let healthchecks = self.healthchecks.resolve().await;

        let handle = HealthCheckHandle::spawn_server(self.config.bind_addr(), healthchecks);
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

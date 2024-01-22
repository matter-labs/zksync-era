use std::fmt;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_core::api_server::healthcheck::HealthCheckHandle;

use crate::{
    implementations::resource::healthcheck::HealthCheckResource,
    node::{NodeContext, StopReceiver},
    resource::ResourceCollection,
    task::{IntoZkSyncTask, TaskInitError, ZkSyncTask},
};

/// Builder for a health check server.
///
/// Spawned task collects all the health checks added by different tasks to the
/// corresponding resource collection and spawns an HTTP server exposing them.
#[derive(Debug)]
pub struct HealthCheckTaskBuilder(pub HealthCheckConfig);

#[async_trait::async_trait]
impl IntoZkSyncTask for HealthCheckTaskBuilder {
    fn task_name(&self) -> &'static str {
        "healthcheck_server"
    }

    async fn create(
        self: Box<Self>,
        mut node: NodeContext<'_>,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let healthchecks = node
            .get_resource_or_default::<ResourceCollection<HealthCheckResource>>()
            .await;

        let task = HealthCheckTask {
            config: self.0,
            healthchecks,
        };

        Ok(Box::new(task))
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
impl ZkSyncTask for HealthCheckTask {
    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let healthchecks = self.healthchecks.resolve().await;

        let handle = HealthCheckHandle::spawn_server(self.config.bind_addr(), healthchecks);
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

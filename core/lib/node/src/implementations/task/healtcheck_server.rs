use std::fmt;

use zksync_config::configs::api::HealthCheckConfig;
use zksync_core::api_server::healthcheck::HealthCheckHandle;

use crate::{
    healthcheck::CheckHealth,
    node::{NodeContext, StopReceiver},
    resource::ResourceCollection,
    task::{IntoZkSyncTask, TaskInitError, ZkSyncTask},
};

pub struct HealthCheckTask {
    config: HealthCheckConfig,
    healthchecks: ResourceCollection<Box<dyn CheckHealth>>,
}

impl HealthCheckTask {
    pub const HEALTHCHECK_COLLECTION_NAME: &'static str = "collection/healthchecks";
}

impl fmt::Debug for HealthCheckTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthCheckTask")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl IntoZkSyncTask for HealthCheckTask {
    type Config = HealthCheckConfig;

    fn create(
        node: &NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let healthchecks =
            node.get_resource_collection::<Box<dyn CheckHealth>>(Self::HEALTHCHECK_COLLECTION_NAME);

        let self_ = Self {
            config,
            healthchecks,
        };

        Ok(Box::new(self_))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for HealthCheckTask {
    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let healthchecks = self.healthchecks.resolve().await?;

        let handle = HealthCheckHandle::spawn_server(self.config.bind_addr(), healthchecks);
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

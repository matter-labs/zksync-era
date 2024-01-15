use crate::{
    healthcheck::{CheckHealth, IntoHealthCheckTask},
    node::NodeContext,
    resource::stop_receiver::StopReceiverResource,
};
use tokio::sync::watch;
use zksync_config::configs::api::HealthCheckConfig;
use zksync_core::api_server::healthcheck::HealthCheckHandle;

use super::{TaskInitError, ZkSyncTask};

pub struct HealthCheckTask {
    config: HealthCheckConfig,
    healthchecks: Vec<Box<dyn CheckHealth>>,
    stop_receiver: watch::Receiver<bool>,
}

impl IntoHealthCheckTask for HealthCheckTask {
    type Config = HealthCheckConfig;

    fn create(
        node: &NodeContext<'_>,
        healthchecks: Vec<Box<dyn CheckHealth>>,
        config: Self::Config,
    ) -> Result<Box<dyn super::ZkSyncTask>, super::TaskInitError> {
        let stop_receiver_resource: StopReceiverResource = node
            .get_resource(StopReceiverResource::RESOURCE_NAME)
            .ok_or_else(|| TaskInitError::ResourceLacking(StopReceiverResource::RESOURCE_NAME))?;
        let self_ = Self {
            config,
            healthchecks,
            stop_receiver: stop_receiver_resource.0,
        };

        Ok(Box::new(self_))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for HealthCheckTask {
    fn healtcheck(&mut self) -> Option<Box<dyn zksync_health_check::CheckHealth>> {
        // Not needed for the healtcheck server.
        None
    }

    async fn run(mut self: Box<Self>) -> anyhow::Result<()> {
        let handle = HealthCheckHandle::spawn_server(self.config.bind_addr(), self.healthchecks);
        self.stop_receiver.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

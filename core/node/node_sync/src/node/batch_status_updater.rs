use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::node::MainNodeClientResource;

use crate::batch_status_updater::BatchStatusUpdater;

#[derive(Debug, FromContext)]
pub struct Input {
    pub pool: PoolResource<MasterPool>,
    pub client: MainNodeClientResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub updater: BatchStatusUpdater,
}

/// Wiring layer for `BatchStatusUpdater`, part of the external node.
#[derive(Debug)]
pub struct BatchStatusUpdaterLayer;

#[async_trait::async_trait]
impl WiringLayer for BatchStatusUpdaterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "batch_status_updater_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            pool,
            client,
            app_health,
        } = input;

        let updater = BatchStatusUpdater::new(client.0, pool.get().await?);

        // Insert healthcheck
        app_health
            .0
            .insert_component(updater.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { updater })
    }
}

#[async_trait::async_trait]
impl Task for BatchStatusUpdater {
    fn id(&self) -> TaskId {
        "batch_status_updater".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

use zksync_reorg_detector::{self, ReorgDetector};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`ReorgDetector`] checker.
/// This layer is responsible for detecting reorgs and shutting down the node if one is detected.
///
/// This layer assumes that the node starts with the initialized state.
#[derive(Debug)]
pub struct ReorgDetectorLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub main_node_client: MainNodeClientResource,
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub reorg_detector: ReorgDetector,
}

#[async_trait::async_trait]
impl WiringLayer for ReorgDetectorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "reorg_detector_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let MainNodeClientResource(main_node_client) = input.main_node_client;
        let pool = input.master_pool.get().await?;

        let reorg_detector = ReorgDetector::new(main_node_client, pool);

        let AppHealthCheckResource(app_health) = input.app_health;
        app_health
            .insert_component(reorg_detector.health_check().clone())
            .map_err(WiringError::internal)?;

        Ok(Output { reorg_detector })
    }
}

#[async_trait::async_trait]
impl Task for ReorgDetector {
    fn id(&self) -> TaskId {
        "reorg_detector".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::try_stoppable;
use zksync_web3_decl::node::MainNodeClientResource;

use crate::ReorgDetector;

/// Wiring layer for [`ReorgDetector`] checker.
/// This layer is responsible for detecting reorgs and shutting down the node if one is detected.
///
/// This layer assumes that the node starts with the initialized state.
#[derive(Debug)]
pub struct ReorgDetectorLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    main_node_client: MainNodeClientResource,
    master_pool: PoolResource<MasterPool>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    reorg_detector: ReorgDetector,
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

        input
            .app_health
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
        try_stoppable!((*self).run(stop_receiver.0).await);
        Ok(())
    }
}

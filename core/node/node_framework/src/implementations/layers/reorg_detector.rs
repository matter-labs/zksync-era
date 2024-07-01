use zksync_reorg_detector::{self, ReorgDetector};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for [`ReorgDetector`] checker.
/// This layer is responsible for detecting reorgs and shutting down the node if one is detected.
///
/// This layer assumes that the node starts with the initialized state.
///
/// ## Requests resources
///
/// - `MainNodeClientResource`
/// - `PoolResource<MasterPool>`
///
/// ## Adds preconditions
///
/// - `ReorgDetector`
#[derive(Debug)]
pub struct ReorgDetectorLayer;

#[async_trait::async_trait]
impl WiringLayer for ReorgDetectorLayer {
    fn layer_name(&self) -> &'static str {
        "reorg_detector_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let main_node_client = context.get_resource::<MainNodeClientResource>().await?.0;

        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let pool = pool_resource.get().await?;

        // Create and insert precondition.
        context.add_task(Box::new(ReorgDetector::new(main_node_client, pool)));

        Ok(())
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

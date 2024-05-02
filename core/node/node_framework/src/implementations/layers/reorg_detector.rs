use std::sync::Arc;

use anyhow::Context;
use zksync_block_reverter::BlockReverter;
use zksync_core::reorg_detector::{self, ReorgDetector};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        reverter::BlockReverterResource,
    },
    precondition::Precondition,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

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

        let reverter = context.get_resource::<BlockReverterResource>().await?.0;

        // Create and insert precondition.
        context.add_precondition(Box::new(ReorgDetectorPrecondition {
            reorg_detector: ReorgDetector::new(main_node_client.clone(), pool.clone()),
            reverter,
        }));

        // Create and insert task.
        context.add_task(Box::new(ReorgDetectorTask {
            reorg_detector: ReorgDetector::new(main_node_client, pool),
        }));

        Ok(())
    }
}

pub struct ReorgDetectorPrecondition {
    reorg_detector: ReorgDetector,
    reverter: Arc<BlockReverter>,
}

#[async_trait::async_trait]
impl Precondition for ReorgDetectorPrecondition {
    fn name(&self) -> &'static str {
        "reorg_detector"
    }

    async fn check(mut self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        match self.reorg_detector.check_consistency().await {
            Ok(()) => {}
            Err(reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
                tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
                self.reverter.roll_back(last_correct_l1_batch).await?;
                tracing::info!("Revert successfully completed");
            }
            Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
        }
        Ok(())
    }
}

pub struct ReorgDetectorTask {
    reorg_detector: ReorgDetector,
}

#[async_trait::async_trait]
impl Task for ReorgDetectorTask {
    fn name(&self) -> &'static str {
        "reorg_detector"
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.reorg_detector
            .run(stop_receiver.0)
            .await
            .context("reorg_detector.run()")
    }
}

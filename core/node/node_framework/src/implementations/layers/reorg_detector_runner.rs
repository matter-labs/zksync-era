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
    service::{ServiceContext, StopReceiver},
    task::UnconstrainedOneshotTask,
    wiring_layer::{WiringError, WiringLayer},
};

/// Layer responsible for detecting reorg and reverting blocks in case it was found.
#[derive(Debug)]
pub struct ReorgDetectorRunnerLayer;

#[async_trait::async_trait]
impl WiringLayer for ReorgDetectorRunnerLayer {
    fn layer_name(&self) -> &'static str {
        "reorg_detector_runner_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let main_node_client = context.get_resource::<MainNodeClientResource>().await?.0;

        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let pool = pool_resource.get().await?;

        let reverter = context.get_resource::<BlockReverterResource>().await?.0;

        // Create and insert task.
        context.add_unconstrained_oneshot_task(Box::new(RunnerUnconstrainedOneshotTask {
            reorg_detector: ReorgDetector::new(main_node_client, pool),
            reverter,
        }));

        Ok(())
    }
}

pub struct RunnerUnconstrainedOneshotTask {
    reorg_detector: ReorgDetector,
    reverter: Arc<BlockReverter>,
}

#[async_trait::async_trait]
impl UnconstrainedOneshotTask for RunnerUnconstrainedOneshotTask {
    fn name(&self) -> &'static str {
        "reorg_detector_runner"
    }

    async fn run_unconstrained_oneshot(
        mut self: Box<Self>,
        _stop_receiver: StopReceiver,
    ) -> anyhow::Result<()> {
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

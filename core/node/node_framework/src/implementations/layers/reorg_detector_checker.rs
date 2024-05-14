use std::time::Duration;

use anyhow::Context;
use zksync_core::reorg_detector::{self, ReorgDetector};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    precondition::Precondition,
    service::{ServiceContext, StopReceiver},
    wiring_layer::{WiringError, WiringLayer},
};

const REORG_DETECTED_SLEEP_INTERVAL: Duration = Duration::from_secs(1);

/// The layer is responsible for integrating reorg checking into the system.
/// When a reorg is detected, the system will not start running until it is fixed.
#[derive(Debug)]
pub struct ReorgDetectorCheckerLayer;

#[async_trait::async_trait]
impl WiringLayer for ReorgDetectorCheckerLayer {
    fn layer_name(&self) -> &'static str {
        "reorg_detector_checker_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let main_node_client = context.get_resource::<MainNodeClientResource>().await?.0;

        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let pool = pool_resource.get().await?;

        // Create and insert precondition.
        context.add_precondition(Box::new(CheckerPrecondition {
            reorg_detector: ReorgDetector::new(main_node_client, pool),
        }));

        Ok(())
    }
}

pub struct CheckerPrecondition {
    reorg_detector: ReorgDetector,
}

#[async_trait::async_trait]
impl Precondition for CheckerPrecondition {
    fn name(&self) -> &'static str {
        "reorg_detector_checker"
    }

    async fn check(mut self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        loop {
            match self.reorg_detector.check_consistency().await {
                Ok(()) => return Ok(()),
                Err(reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
                    tracing::warn!(
                        "Reorg detected, last correct L1 batch #{}. Waiting till it will be resolved. Sleep for {} seconds and retry",
                        last_correct_l1_batch, REORG_DETECTED_SLEEP_INTERVAL.as_secs()
                    );
                    tokio::time::sleep(REORG_DETECTED_SLEEP_INTERVAL).await;
                }
                Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
            }
        }
    }
}

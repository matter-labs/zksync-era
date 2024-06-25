use std::time::Duration;

use anyhow::Context;
use zksync_dal::{ConnectionPool, Core};
use zksync_reorg_detector::{self, ReorgDetector};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

const REORG_DETECTED_SLEEP_INTERVAL: Duration = Duration::from_secs(1);

/// Wiring layer for [`ReorgDetector`] checker.
/// This layer is responsible for detecting reorgs and preventing the node from starting if it occurs.
///
/// ## Requests resources
///
/// - `MainNodeClientResource`
/// - `PoolResource<MasterPool>`
///
/// ## Adds preconditions
///
/// - `CheckerPrecondition`
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
        context.add_task(Box::new(CheckerPrecondition {
            pool: pool.clone(),
            reorg_detector: ReorgDetector::new(main_node_client, pool),
        }));

        Ok(())
    }
}

pub struct CheckerPrecondition {
    pool: ConnectionPool<Core>,
    reorg_detector: ReorgDetector,
}

#[async_trait::async_trait]
impl Task for CheckerPrecondition {
    fn kind(&self) -> TaskKind {
        TaskKind::Precondition
    }

    fn id(&self) -> TaskId {
        "reorg_detector_checker".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Given that this is a precondition -- i.e. something that starts before some invariants are met,
        // we need to first ensure that there is at least one batch in the database (there may be none if
        // either genesis or snapshot recovery has not been performed yet).
        let earliest_batch = zksync_dal::helpers::wait_for_l1_batch(
            &self.pool,
            REORG_DETECTED_SLEEP_INTERVAL,
            &mut stop_receiver.0,
        )
        .await?;
        if earliest_batch.is_none() {
            // Stop signal received.
            return Ok(());
        }

        loop {
            match self.reorg_detector.run_once(stop_receiver.0.clone()).await {
                Ok(()) => return Ok(()),
                Err(zksync_reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
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

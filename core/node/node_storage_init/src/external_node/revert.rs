use anyhow::Context as _;
use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_reorg_detector::ReorgDetector;
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{DynClient, L2};

use crate::RevertStorage;

#[derive(Debug)]
pub struct ExternalNodeReverter {
    pub client: Box<DynClient<L2>>,
    pub pool: ConnectionPool<Core>,
    pub reverter: Option<BlockReverter>,
}

#[async_trait::async_trait]
impl RevertStorage for ExternalNodeReverter {
    async fn revert_storage(
        &self,
        to_batch: L1BatchNumber,
        _stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let Some(block_reverter) = self.reverter.as_ref() else {
            anyhow::bail!(
                "Revert to block {to_batch} was requested, but the reverter was not provided."
            );
        };

        tracing::info!("Reverting to l1 batch number {to_batch}");
        block_reverter.roll_back(to_batch).await?;
        tracing::info!("Revert successfully completed");
        Ok(())
    }

    async fn last_correct_batch_for_reorg(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut reorg_detector = ReorgDetector::new(self.client.clone(), self.pool.clone());
        let batch = match reorg_detector.run_once(stop_receiver).await {
            Ok(()) => {
                // Even if stop signal was received, the node will shut down without launching any tasks.
                tracing::info!("No rollback was detected");
                None
            }
            Err(zksync_reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
                tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
                Some(last_correct_l1_batch)
            }
            Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
        };
        Ok(batch)
    }
}

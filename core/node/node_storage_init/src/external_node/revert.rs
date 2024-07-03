use anyhow::Context as _;
use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_reorg_detector::ReorgDetector;
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{DynClient, L2};

use crate::RevertStorage;

#[derive(Debug)]
pub struct ExternalNodeRevert {
    pub client: Box<DynClient<L2>>,
    pub pool: ConnectionPool<Core>,
    pub reverter: BlockReverter,
}

#[async_trait::async_trait]
impl RevertStorage for ExternalNodeRevert {
    async fn revert_storage(
        &self,
        to_batch: L1BatchNumber,
        _stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        tracing::info!("Reverting to l1 batch number {to_batch}");
        self.reverter.roll_back(to_batch).await?;
        tracing::info!("Revert successfully completed");
        Ok(())
    }

    async fn is_revert_needed(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut reorg_detector = ReorgDetector::new(self.client.clone(), self.pool.clone());
        // We don't need this kind of granularity for detecting stop signal; it's expected to be treated
        // by the caller.
        let (_mock_tx, mock_rx) = watch::channel(false);
        let batch = match reorg_detector.run_once(mock_rx).await {
            Ok(_) => {
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

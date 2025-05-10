use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_reorg_detector::{Error as ReorgError, ReorgDetector};
use zksync_types::{L1BatchNumber, OrStopped};
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
    async fn revert_storage(&self, to_batch: L1BatchNumber) -> anyhow::Result<()> {
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

    async fn is_reorg_needed(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<bool, OrStopped> {
        ReorgDetector::new(self.client.clone(), self.pool.clone())
            .check_reorg_presence(stop_receiver)
            .await
    }

    async fn last_correct_batch_for_reorg(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<Option<L1BatchNumber>, OrStopped> {
        let mut reorg_detector = ReorgDetector::new(self.client.clone(), self.pool.clone());
        match reorg_detector.run_once(stop_receiver).await {
            Ok(()) => {
                tracing::info!("No rollback was detected");
                Ok(None)
            }
            Err(OrStopped::Internal(ReorgError::ReorgDetected(last_correct_l1_batch))) => {
                tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
                Ok(Some(last_correct_l1_batch))
            }
            Err(OrStopped::Internal(err)) => Err(OrStopped::Internal(err.into())),
            Err(OrStopped::Stopped) => Err(OrStopped::Stopped),
        }
    }
}

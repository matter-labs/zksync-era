use std::{fmt, future::Future, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::ConnectionPool;
use zksync_types::{L1BatchNumber, MiniblockNumber, H256};
use zksync_web3_decl::{
    jsonrpsee::{
        core::ClientError as RpcError,
        http_client::{HttpClient, HttpClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::{
    metrics::{CheckerComponent, EN_METRICS},
    utils::wait_for_l1_batch_with_metadata,
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum HashMatchError {
    #[error("RPC error calling main node")]
    Rpc(#[from] RpcError),
    #[error(
        "Unrecoverable error: the earliest L1 batch #{0} in the local DB \
        has mismatched hash with the main node. Make sure you're connected to the right network; \
        if you've recovered from a snapshot, re-check snapshot authenticity. \
        Using an earlier snapshot could help."
    )]
    EarliestHashMismatch(L1BatchNumber),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::SqlxError> for HashMatchError {
    fn from(err: zksync_dal::SqlxError) -> Self {
        Self::Internal(err.into())
    }
}

fn is_transient_err(err: &RpcError) -> bool {
    matches!(err, RpcError::Transport(_) | RpcError::RequestTimeout)
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn miniblock_hash(&self, number: MiniblockNumber) -> Result<Option<H256>, RpcError>;

    async fn l1_batch_root_hash(&self, number: L1BatchNumber) -> Result<Option<H256>, RpcError>;
}

#[async_trait]
impl MainNodeClient for HttpClient {
    async fn miniblock_hash(&self, number: MiniblockNumber) -> Result<Option<H256>, RpcError> {
        Ok(self
            .get_block_by_number(number.0.into(), false)
            .await?
            .map(|block| block.hash))
    }

    async fn l1_batch_root_hash(&self, number: L1BatchNumber) -> Result<Option<H256>, RpcError> {
        Ok(self
            .get_l1_batch_details(number)
            .await?
            .and_then(|batch| batch.base.root_hash))
    }
}

trait UpdateCorrectBlock: fmt::Debug + Send + Sync {
    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    );
}

/// Default implementation of [`UpdateCorrectBlock`] that reports values as metrics.
impl UpdateCorrectBlock for () {
    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        EN_METRICS.last_correct_batch[&CheckerComponent::ReorgDetector]
            .set(last_correct_miniblock.0.into());
        EN_METRICS.last_correct_miniblock[&CheckerComponent::ReorgDetector]
            .set(last_correct_l1_batch.0.into());
    }
}

/// This is a component that is responsible for detecting the batch re-orgs.
/// Batch re-org is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof), and then potentially
/// re-organize transactions in them to fix the problem.
///
/// To detect them, we constantly check the latest sealed batch root hash,
/// and in the event of mismatch, we know that there has been a re-org.
/// We then perform a binary search to find the latest correct block
/// and revert all batches after it, to keep being consistent with the main node.
///
/// This is the only component that is expected to finish its execution
/// in the even of re-org, since we have to restart the node after a rollback is performed,
/// and is special-cased in the `zksync_external_node` crate.
#[derive(Debug)]
pub struct ReorgDetector {
    client: Box<dyn MainNodeClient>,
    block_updater: Box<dyn UpdateCorrectBlock>,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    sleep_interval: Duration,
}

impl ReorgDetector {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(url: &str, pool: ConnectionPool, stop_receiver: watch::Receiver<bool>) -> Self {
        let client = HttpClientBuilder::default()
            .build(url)
            .expect("Failed to create HTTP client");
        Self {
            client: Box::new(client),
            block_updater: Box::new(()),
            pool,
            stop_receiver,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
        }
    }

    /// Compares hashes of the given local miniblock and the same miniblock from main node.
    async fn miniblock_hashes_match(
        &self,
        miniblock_number: MiniblockNumber,
    ) -> Result<bool, HashMatchError> {
        let mut storage = self.pool.access_storage().await?;
        let local_hash = storage
            .blocks_dal()
            .get_miniblock_header(miniblock_number)
            .await?
            .with_context(|| {
                format!("Header does not exist for local miniblock #{miniblock_number}")
            })?
            .hash;
        drop(storage);

        let Some(remote_hash) = self.client.miniblock_hash(miniblock_number).await? else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(true);
        };

        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local hash {local_hash:?} doesn't match the hash from \
                main node {remote_hash:?} (miniblock #{miniblock_number})"
            );
        }
        Ok(remote_hash == local_hash)
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn root_hashes_match(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<bool, HashMatchError> {
        let mut storage = self.pool.access_storage().await?;
        let local_hash = storage
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number)
            .await?
            .with_context(|| {
                format!("Root hash does not exist for local batch #{l1_batch_number}")
            })?;
        drop(storage);

        let Some(remote_hash) = self.client.l1_batch_root_hash(l1_batch_number).await? else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the root hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(true);
        };

        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local root hash {local_hash:?} doesn't match the state hash from \
                main node {remote_hash:?} (L1 batch #{l1_batch_number})"
            );
        }
        Ok(remote_hash == local_hash)
    }

    /// Localizes a re-org: performs binary search to determine the last non-diverged block.
    async fn detect_reorg(
        &self,
        known_valid_l1_batch: L1BatchNumber,
        diverged_l1_batch: L1BatchNumber,
    ) -> Result<L1BatchNumber, HashMatchError> {
        // TODO (BFT-176, BFT-181): We have to look through the whole history, since batch status updater may mark
        //   a block as executed even if the state diverges for it.
        binary_search_with(known_valid_l1_batch.0, diverged_l1_batch.0, |number| {
            self.root_hashes_match(L1BatchNumber(number))
        })
        .await
        .map(L1BatchNumber)
    }

    pub async fn run(mut self) -> anyhow::Result<Option<L1BatchNumber>> {
        loop {
            match self.run_inner().await {
                Ok(l1_batch_number) => return Ok(l1_batch_number),
                Err(HashMatchError::Rpc(err)) if is_transient_err(&err) => {
                    tracing::warn!("Following transport error occurred: {err}");
                    tracing::info!("Trying again after a delay");
                    tokio::time::sleep(self.sleep_interval).await;
                }
                Err(HashMatchError::Internal(err)) => return Err(err),
                Err(err) => return Err(err.into()),
            }
        }
    }

    async fn run_inner(&mut self) -> Result<Option<L1BatchNumber>, HashMatchError> {
        let earliest_l1_batch_number = wait_for_l1_batch_with_metadata(
            &self.pool,
            self.sleep_interval,
            &mut self.stop_receiver,
        )
        .await?;

        let Some(earliest_l1_batch_number) = earliest_l1_batch_number else {
            return Ok(None); // Stop signal received
        };
        tracing::debug!(
            "Checking root hash match for earliest L1 batch #{earliest_l1_batch_number}"
        );
        if !self.root_hashes_match(earliest_l1_batch_number).await? {
            return Err(HashMatchError::EarliestHashMismatch(
                earliest_l1_batch_number,
            ));
        }

        loop {
            let should_stop = *self.stop_receiver.borrow();

            // At this point, we are guaranteed to have L1 batches and miniblocks in the storage.
            let mut storage = self.pool.access_storage().await?;
            let sealed_l1_batch_number = storage
                .blocks_dal()
                .get_last_l1_batch_number_with_metadata()
                .await?
                .context("L1 batches table unexpectedly emptied")?;
            let sealed_miniblock_number = storage
                .blocks_dal()
                .get_sealed_miniblock_number()
                .await?
                .context("miniblocks table unexpectedly emptied")?;
            drop(storage);

            tracing::trace!(
                "Checking for reorgs - L1 batch #{sealed_l1_batch_number}, \
                 miniblock number #{sealed_miniblock_number}"
            );

            let root_hashes_match = self.root_hashes_match(sealed_l1_batch_number).await?;
            let miniblock_hashes_match =
                self.miniblock_hashes_match(sealed_miniblock_number).await?;

            // The only event that triggers re-org detection and node rollback is if the
            // hash mismatch at the same block height is detected, be it miniblocks or batches.
            //
            // In other cases either there is only a height mismatch which means that one of
            // the nodes needs to do catching up; however, it is not certain that there is actually
            // a re-org taking place.
            if root_hashes_match && miniblock_hashes_match {
                self.block_updater
                    .update_correct_block(sealed_miniblock_number, sealed_l1_batch_number);
            } else {
                let diverged_l1_batch_number = if root_hashes_match {
                    sealed_l1_batch_number + 1 // Non-sealed L1 batch has diverged
                } else {
                    sealed_l1_batch_number
                };

                tracing::info!("Searching for the first diverged L1 batch");
                let last_correct_l1_batch = self
                    .detect_reorg(earliest_l1_batch_number, diverged_l1_batch_number)
                    .await?;
                tracing::info!(
                    "Reorg localized: last correct L1 batch is #{last_correct_l1_batch}"
                );
                return Ok(Some(last_correct_l1_batch));
            }

            if should_stop {
                tracing::info!("Shutting down reorg detector");
                return Ok(None);
            }
            tokio::time::sleep(self.sleep_interval).await;
        }
    }
}

async fn binary_search_with<F, Fut, E>(mut left: u32, mut right: u32, mut f: F) -> Result<u32, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<bool, E>>,
{
    while left + 1 < right {
        let middle = (left + right) / 2;
        assert!(middle < right); // middle <= (right - 2 + right) / 2 = right - 1

        if f(middle).await? {
            left = middle;
        } else {
            right = middle;
        }
    }
    Ok(left)
}

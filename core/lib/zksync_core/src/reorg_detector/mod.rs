use std::{future::Future, time::Duration};

use tokio::sync::watch;
use zksync_dal::ConnectionPool;
use zksync_types::{L1BatchNumber, MiniblockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::Error as RpcError,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    RpcResult,
};

use crate::metrics::{CheckerComponent, EN_METRICS};

const SLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// This is a component that is responsible for detecting the batch reorgs.
/// Batch reorg is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof), and then potentially
/// re-organize transactions in them to fix the problem.
///
/// To detect them, we constantly check the latest sealed batch root hash,
/// and in the event of mismatch, we know that there has been a reorg.
/// We then perform a binary search to find the latest correct block
/// and revert all batches after it, to keep being consistent with the main node.
///
/// This is the only component that is expected to finish its execution
/// in the even of reorg, since we have to restart the node after a rollback is performed,
/// and is special-cased in the `zksync_external_node` crate.
#[derive(Debug)]
pub struct ReorgDetector {
    client: HttpClient,
    pool: ConnectionPool,
    should_stop: watch::Receiver<bool>,
}

impl ReorgDetector {
    pub fn new(url: &str, pool: ConnectionPool, should_stop: watch::Receiver<bool>) -> Self {
        let client = HttpClientBuilder::default()
            .build(url)
            .expect("Failed to create HTTP client");
        Self {
            client,
            pool,
            should_stop,
        }
    }

    /// Compares hashes of the given local miniblock and the same miniblock from main node.
    async fn miniblock_hashes_match(&self, miniblock_number: MiniblockNumber) -> RpcResult<bool> {
        let local_hash = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .blocks_dal()
            .get_miniblock_header(miniblock_number)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "Header does not exist for local miniblock #{}",
                    miniblock_number
                )
            })
            .hash;

        let Some(hash) = self
            .client
            .get_block_by_number(miniblock_number.0.into(), false)
            .await?
            .map(|header| header.hash)
        else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(true);
        };

        Ok(hash == local_hash)
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn root_hashes_match(&self, l1_batch_number: L1BatchNumber) -> RpcResult<bool> {
        // Unwrapping is fine since the caller always checks that these root hashes exist.
        let local_hash = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "Root hash does not exist for local batch #{}",
                    l1_batch_number
                )
            });
        let Some(hash) = self
            .client
            .get_l1_batch_details(l1_batch_number)
            .await?
            .and_then(|b| b.base.root_hash)
        else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the root hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(true);
        };
        Ok(hash == local_hash)
    }

    /// Localizes a reorg: performs binary search to determine the last non-diverged block.
    async fn detect_reorg(&self, diverged_l1_batch: L1BatchNumber) -> RpcResult<L1BatchNumber> {
        // TODO (BFT-176, BFT-181): We have to look through the whole history, since batch status updater may mark
        // a block as executed even if the state diverges for it.
        binary_search_with(1, diverged_l1_batch.0, |number| {
            self.root_hashes_match(L1BatchNumber(number))
        })
        .await
        .map(L1BatchNumber)
    }

    pub async fn run(mut self) -> Option<L1BatchNumber> {
        loop {
            match self.run_inner().await {
                Ok(l1_batch_number) => return l1_batch_number,
                Err(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) => {
                    tracing::warn!("Following transport error occurred: {err}");
                    tracing::info!("Trying again after a delay");
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                }
                Err(err) => {
                    panic!("Unexpected error in the reorg detector: {}", err);
                }
            }
        }
    }

    async fn run_inner(&mut self) -> RpcResult<Option<L1BatchNumber>> {
        loop {
            let should_stop = *self.should_stop.borrow();

            let sealed_l1_batch_number = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_last_l1_batch_number_with_metadata()
                .await
                .unwrap();

            let sealed_miniblock_number = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_sealed_miniblock_number()
                .await
                .unwrap();

            tracing::trace!(
                "Checking for reorgs - L1 batch #{sealed_l1_batch_number}, \
                miniblock number #{sealed_miniblock_number}"
            );

            let root_hashes_match = self.root_hashes_match(sealed_l1_batch_number).await?;
            let miniblock_hashes_match =
                self.miniblock_hashes_match(sealed_miniblock_number).await?;

            // The only event that triggers reorg detection and node rollback is if the
            // hash mismatch at the same block height is detected, be it miniblocks or batches.
            //
            // In other cases either there is only a height mismatch which means that one of
            // the nodes needs to do catching up, howver it is not certain that there is actually
            // a reorg taking place.
            if root_hashes_match && miniblock_hashes_match {
                EN_METRICS.last_correct_batch[&CheckerComponent::ReorgDetector]
                    .set(sealed_l1_batch_number.0.into());
                EN_METRICS.last_correct_miniblock[&CheckerComponent::ReorgDetector]
                    .set(sealed_miniblock_number.0.into());
            } else {
                if !root_hashes_match {
                    tracing::warn!(
                        "Reorg detected: last state hash doesn't match the state hash from \
                        main node (L1 batch #{sealed_l1_batch_number})"
                    );
                }
                if !miniblock_hashes_match {
                    tracing::warn!(
                        "Reorg detected: last state hash doesn't match the state hash from \
                        main node (MiniblockNumber #{sealed_miniblock_number})"
                    );
                }
                tracing::info!("Searching for the first diverged batch");
                let last_correct_l1_batch = self.detect_reorg(sealed_l1_batch_number).await?;
                tracing::info!(
                    "Reorg localized: last correct L1 batch is #{last_correct_l1_batch}"
                );
                return Ok(Some(last_correct_l1_batch));
            }
            if should_stop {
                tracing::info!("Shutting down reorg detector");
                return Ok(None);
            }
            tokio::time::sleep(SLEEP_INTERVAL).await;
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
        if f(middle).await? {
            left = middle;
        } else {
            right = middle;
        }
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    /// Tests the binary search algorithm.
    #[tokio::test]
    async fn test_binary_search() {
        for divergence_point in [1, 50, 51, 100] {
            let mut f = |x| async move { Ok::<_, ()>(x < divergence_point) };
            let result = super::binary_search_with(0, 100, &mut f).await;
            assert_eq!(result, Ok(divergence_point - 1));
        }
    }
}

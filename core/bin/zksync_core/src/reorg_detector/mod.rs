use zksync_dal::ConnectionPool;
use zksync_types::L1BatchNumber;
use zksync_web3_decl::{
    jsonrpsee::core::Error as RpcError,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::ZksNamespaceClient,
    RpcResult,
};

use std::{future::Future, time::Duration};

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
}

impl ReorgDetector {
    pub fn new(url: &str, pool: ConnectionPool) -> Self {
        let client = HttpClientBuilder::default()
            .build(url)
            .expect("Failed to create HTTP client");
        Self { client, pool }
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn root_hashes_match(&self, l1_batch_number: L1BatchNumber) -> RpcResult<bool> {
        // Unwrapping is fine since the caller always checks that these root hashes exist.
        let local_hash = self
            .pool
            .access_storage()
            .await
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number)
            .await
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
            // Lack of the root hash on the main node is treated as a hash mismatch,
            // so we can continue searching for the last correct block.
            return Ok(false);
        };
        Ok(hash == local_hash)
    }

    /// Localizes a reorg: performs binary search to determine the last non-diverged block.
    async fn detect_reorg(&self, diverged_l1_batch: L1BatchNumber) -> RpcResult<L1BatchNumber> {
        binary_search_with(1, diverged_l1_batch.0, |number| {
            self.root_hashes_match(L1BatchNumber(number))
        })
        .await
        .map(L1BatchNumber)
    }

    pub async fn run(self) -> L1BatchNumber {
        loop {
            match self.run_inner().await {
                Ok(l1_batch_number) => return l1_batch_number,
                Err(err @ RpcError::Transport(_) | err @ RpcError::RequestTimeout) => {
                    vlog::warn!("Following transport error occurred: {err}");
                    vlog::info!("Trying again after a delay");
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                }
                Err(err) => {
                    panic!("Unexpected error in the reorg detector: {}", err);
                }
            }
        }
    }

    /// Checks if the external node is ahead of the main node *NOT* because of a reorg.
    /// In such an event, we should not do anything.
    ///
    /// Theoretically, external node might calculate batch root hash before the main
    /// node. Therefore, we need to be sure that we check a batch which has root hashes
    /// both on the main node and on the external node.
    async fn is_legally_ahead_of_main_node(
        &self,
        sealed_l1_batch_number: L1BatchNumber,
    ) -> RpcResult<bool> {
        // We must know the latest batch on the main node *before* we ask it for a root hash
        // to prevent a race condition (asked for root hash, batch sealed on main node, we've got
        // inconsistent results).
        let last_main_node_l1_batch = self.client.get_l1_batch_number().await?;
        let main_node_l1_batch_root_hash = self
            .client
            .get_l1_batch_details(sealed_l1_batch_number)
            .await?
            .and_then(|b| b.base.root_hash);

        let en_ahead_for = sealed_l1_batch_number
            .0
            .checked_sub(last_main_node_l1_batch.as_u32());
        // Theoretically it's possible that the EN would not only calculate the root hash, but also seal the batch
        // quicker than the main node. So, we allow us to be at most one batch ahead of the main node.
        // If the gap is bigger, it's certainly a reorg.
        // Allowing the gap is safe: if reorg has happened, it'll be detected anyway in the future iterations.
        Ok(main_node_l1_batch_root_hash.is_none() && en_ahead_for <= Some(1))
    }

    async fn run_inner(&self) -> RpcResult<L1BatchNumber> {
        loop {
            let sealed_l1_batch_number = self
                .pool
                .access_storage()
                .await
                .blocks_dal()
                .get_last_l1_batch_number_with_metadata()
                .await;

            // If the main node has to catch up with us, we should not do anything just yet.
            if self
                .is_legally_ahead_of_main_node(sealed_l1_batch_number)
                .await?
            {
                vlog::trace!(
                    "Local state was updated ahead of the main node. Waiting for the main node to seal the batch"
                );
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            }

            // At this point we're certain that if we detect a reorg, it's real.
            vlog::trace!("Checking for reorgs - L1 batch #{sealed_l1_batch_number}");
            if self.root_hashes_match(sealed_l1_batch_number).await? {
                metrics::gauge!(
                    "external_node.last_correct_batch",
                    sealed_l1_batch_number.0 as f64,
                    "component" => "reorg_detector",
                );
                tokio::time::sleep(SLEEP_INTERVAL).await;
            } else {
                vlog::warn!(
                    "Reorg detected: last state hash doesn't match the state hash from main node \
                     (L1 batch #{sealed_l1_batch_number})"
                );
                vlog::info!("Searching for the first diverged batch");
                let last_correct_l1_batch = self.detect_reorg(sealed_l1_batch_number).await?;
                vlog::info!("Reorg localized: last correct L1 batch is #{last_correct_l1_batch}");
                return Ok(last_correct_l1_batch);
            }
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

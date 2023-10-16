use std::collections::HashMap;

use zksync_types::{api::en::SyncBlock, MiniblockNumber, U64};
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EnNamespaceClient, EthNamespaceClient},
    RpcResult,
};

use super::metrics::{CachedMethod, FETCHER_METRICS};

/// Maximum number of concurrent requests to the main node.
const MAX_CONCURRENT_REQUESTS: usize = 100;

/// This is a temporary implementation of a cache layer for the main node HTTP requests.
/// It was introduced to quickly develop a way to fetch data from the main node concurrently,
/// while not changing the logic of the fetcher itself.
/// It is intentionally designed in an "easy-to-inject, easy-to-remove" way, so that we can easily
/// switch it to a more performant implementation later.
///
/// The main part of this structure's logic is the ability to concurrently populate the cache
/// of responses and then consume them in a non-concurrent way.
///
/// Note: not every request is guaranted cached, only the ones that are used to build the action queue.
/// For example, if batch status updater requests a miniblock header long after it was processed by the main
/// fetcher routine, most likely it'll be a cache miss.
#[derive(Debug)]
pub(super) struct CachedMainNodeClient {
    /// HTTP client.
    client: HttpClient,
    /// Earliest miniblock number that is not yet cached.
    /// Used as a marker to refill the cache.
    next_refill_at: MiniblockNumber,
    blocks: HashMap<MiniblockNumber, SyncBlock>,
}

impl CachedMainNodeClient {
    pub fn build_client(main_node_url: &str) -> Self {
        let client = HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client");
        Self {
            client,
            next_refill_at: MiniblockNumber(0),
            blocks: Default::default(),
        }
    }

    /// Cached version of [`HttpClient::sync_l2_block`].
    pub async fn sync_l2_block(&self, miniblock: MiniblockNumber) -> RpcResult<Option<SyncBlock>> {
        let block = self.blocks.get(&miniblock).cloned();
        FETCHER_METRICS.cache_total[&CachedMethod::SyncL2Block].inc();
        match block {
            Some(block) => {
                FETCHER_METRICS.cache_hit[&CachedMethod::SyncL2Block].inc();
                Ok(Some(block))
            }
            None => self.client.sync_l2_block(miniblock, true).await,
        }
    }

    /// Re-export of [`HttpClient::get_block_number`].
    /// Added to not expose the internal client.
    pub async fn get_block_number(&self) -> RpcResult<U64> {
        self.client.get_block_number().await
    }

    /// Removes a miniblock data from the cache.
    pub fn forget_miniblock(&mut self, miniblock: MiniblockNumber) {
        self.blocks.remove(&miniblock);
    }

    pub async fn populate_miniblocks_cache(
        &mut self,
        current_miniblock: MiniblockNumber,
        last_miniblock: MiniblockNumber,
    ) {
        // This method may be invoked frequently, but in order to take advantage of the concurrent fetching,
        // we only need to do it once in a while. If we'll do it too often, we'll end up adding 1 element to
        // the cache at a time, which eliminates the cache's purpose.
        if current_miniblock < self.next_refill_at {
            return;
        }
        let populate_latency = FETCHER_METRICS.cache_populate.start();
        let last_miniblock_to_fetch =
            last_miniblock.min(current_miniblock + MAX_CONCURRENT_REQUESTS as u32);
        let task_futures = (current_miniblock.0..last_miniblock_to_fetch.0)
            .map(MiniblockNumber)
            .filter(|&miniblock| {
                // If the miniblock is already in the cache, we don't need to fetch it.
                !self.has_miniblock(miniblock)
            })
            .map(|block_number| self.client.sync_l2_block(block_number, true));

        let results = futures::future::join_all(task_futures).await;
        for result in results {
            if let Ok(Some(block)) = result {
                self.next_refill_at = self.next_refill_at.max(block.number + 1);
                self.blocks.insert(block.number, block);
            } else {
                // At the cache level, it's fine to just silence errors.
                // The entry won't be included into the cache, and whoever uses the cache, will have to process
                // a cache miss as they will.
                FETCHER_METRICS.cache_errors.inc();
            }
        }
        populate_latency.observe();
    }

    fn has_miniblock(&self, miniblock: MiniblockNumber) -> bool {
        self.blocks.contains_key(&miniblock)
    }
}

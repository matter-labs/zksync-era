use std::{collections::HashMap, time::Instant};

use zksync_types::{explorer_api::BlockDetails, L1BatchNumber, MiniblockNumber, Transaction, U64};
use zksync_web3_decl::{
    jsonrpsee::{
        core::RpcResult,
        http_client::{HttpClient, HttpClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

/// Maximum number of concurrent requests to the main node.
const MAX_CONCURRENT_REQUESTS: usize = 100;
/// Set of fields fetched together for a single miniblock.
type MiniblockData = (BlockDetails, Option<(U64, U64)>, Vec<Transaction>);

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
    miniblock_headers: HashMap<MiniblockNumber, BlockDetails>,
    batch_ranges: HashMap<L1BatchNumber, (U64, U64)>,
    txs: HashMap<MiniblockNumber, Vec<Transaction>>,
}

impl CachedMainNodeClient {
    pub fn build_client(main_node_url: &str) -> Self {
        let client = HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client");
        Self {
            client,
            next_refill_at: MiniblockNumber(0),
            miniblock_headers: Default::default(),
            batch_ranges: Default::default(),
            txs: Default::default(),
        }
    }

    /// Cached version of [`HttpClient::get_raw_block_transaction`].
    pub async fn get_raw_block_transactions(
        &self,
        miniblock: MiniblockNumber,
    ) -> RpcResult<Vec<Transaction>> {
        let txs = { self.txs.get(&miniblock).cloned() };
        metrics::increment_counter!("external_node.fetcher.cache.total", "method" => "get_raw_block_transactions");
        match txs {
            Some(txs) => {
                metrics::increment_counter!("external_node.fetcher.cache.hit", "method" => "get_raw_block_transactions");
                Ok(txs)
            }
            None => self.client.get_raw_block_transactions(miniblock).await,
        }
    }

    /// Cached version of [`HttpClient::get_block_range`].
    pub async fn get_block_details(
        &self,
        miniblock: MiniblockNumber,
    ) -> RpcResult<Option<BlockDetails>> {
        let block_details = self.miniblock_headers.get(&miniblock).cloned();
        metrics::increment_counter!("external_node.fetcher.cache.total", "method" => "get_block_details");
        match block_details {
            Some(block_details) => {
                metrics::increment_counter!("external_node.fetcher.cache.hit", "method" => "get_block_details");
                Ok(Some(block_details))
            }
            None => self.client.get_block_details(miniblock).await,
        }
    }

    /// Cached version of [`HttpClient::get_miniblock_range`].
    pub async fn get_miniblock_range(&self, batch: L1BatchNumber) -> RpcResult<Option<(U64, U64)>> {
        let range = self.batch_ranges.get(&batch).cloned();
        metrics::increment_counter!("external_node.fetcher.cache.total", "method" => "get_miniblock_range");
        match range {
            Some(range) => {
                metrics::increment_counter!("external_node.fetcher.cache.hit", "method" => "get_miniblock_range");
                Ok(Some(range))
            }
            None => self.client.get_miniblock_range(batch).await,
        }
    }

    /// Re-export of [`HttpClient::get_block_number`].
    /// Added to not expose the internal client.
    pub async fn get_block_number(&self) -> RpcResult<U64> {
        self.client.get_block_number().await
    }

    /// Removes a miniblock data from the cache.
    pub fn forget_miniblock(&mut self, miniblock: MiniblockNumber) {
        self.miniblock_headers.remove(&miniblock);
        self.txs.remove(&miniblock);
    }

    pub fn forget_l1_batch(&mut self, l1_batch: L1BatchNumber) {
        self.batch_ranges.remove(&l1_batch);
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
        let start = Instant::now();
        let last_miniblock_to_fetch =
            last_miniblock.min(current_miniblock + MAX_CONCURRENT_REQUESTS as u32);
        let task_futures = (current_miniblock.0..last_miniblock_to_fetch.0)
            .map(MiniblockNumber)
            .filter(|&miniblock| {
                // If the miniblock is already in the cache, we don't need to fetch it.
                !self.has_miniblock(miniblock)
            })
            .map(|miniblock| Self::fetch_one_miniblock(&self.client, miniblock));

        let results = futures::future::join_all(task_futures).await;
        for result in results {
            if let Ok(Some((header, range, txs))) = result {
                let miniblock = header.number;
                let batch = header.l1_batch_number;
                self.miniblock_headers.insert(miniblock, header);
                if let Some(range) = range {
                    self.batch_ranges.insert(batch, range);
                }
                self.txs.insert(miniblock, txs);
                self.next_refill_at = self.next_refill_at.max(miniblock + 1);
            } else {
                // At the cache level, it's fine to just silence errors.
                // The entry won't be included into the cache, and whoever uses the cache, will have to process
                // a cache miss as they will.
                metrics::increment_counter!("external_node.fetcher.cache.errors");
            }
        }
        metrics::histogram!("external_node.fetcher.cache.populate", start.elapsed());
    }

    fn has_miniblock(&self, miniblock: MiniblockNumber) -> bool {
        self.miniblock_headers.contains_key(&miniblock)
    }

    async fn fetch_one_miniblock(
        client: &HttpClient,
        miniblock: MiniblockNumber,
    ) -> RpcResult<Option<MiniblockData>> {
        // Error propagation here would mean that these entries won't appear in the cache.
        // This would cause a cache miss, but generally it shouldn't be a problem as long as the API errors are rare.
        // If the API returns lots of errors, that's a problem regardless of caching.
        let start = Instant::now();
        let header = client.get_block_details(miniblock).await;
        metrics::histogram!("external_node.fetcher.cache.requests", start.elapsed(), "stage" => "get_block_details");
        let Some(header) = header? else { return Ok(None) };

        let start = Instant::now();
        let miniblock_range = client.get_miniblock_range(header.l1_batch_number).await?;
        metrics::histogram!("external_node.fetcher.cache.requests", start.elapsed(), "stage" => "get_miniblock_range");

        let start = Instant::now();
        let miniblock_txs = client.get_raw_block_transactions(miniblock).await?;
        metrics::histogram!("external_node.fetcher.cache.requests", start.elapsed(), "stage" => "get_raw_block_transactions");

        Ok(Some((header, miniblock_range, miniblock_txs)))
    }
}

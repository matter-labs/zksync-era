//! Client abstractions for syncing between the external node and the main node.

use std::{collections::HashMap, fmt};

use async_trait::async_trait;
use zksync_system_constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_types::{
    api::{self, en::SyncBlock},
    get_code_key, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256, U64,
};
use zksync_web3_decl::{
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
};

use super::metrics::{CachedMethod, FETCHER_METRICS};

/// Maximum number of concurrent requests to the main node.
const MAX_CONCURRENT_REQUESTS: usize = 100;

/// Client abstracting connection to the main node.
#[async_trait]
pub trait MainNodeClient: 'static + Send + Sync + fmt::Debug {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    async fn fetch_genesis_contract_bytecode(
        &self,
        address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersion>>;

    async fn fetch_genesis_l1_batch_hash(&self) -> EnrichedClientResult<H256>;

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<MiniblockNumber>;

    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<SyncBlock>>;
}

impl dyn MainNodeClient {
    /// Creates a client based on JSON-RPC.
    pub fn json_rpc(url: &str) -> anyhow::Result<HttpClient> {
        HttpClientBuilder::default().build(url).map_err(Into::into)
    }
}

#[async_trait]
impl MainNodeClient for HttpClient {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let bytecode = self
            .get_bytecode_by_hash(hash)
            .rpc_context("get_bytecode_by_hash")
            .with_arg("hash", &hash)
            .await?;
        if let Some(bytecode) = &bytecode {
            let actual_bytecode_hash = zksync_utils::bytecode::hash_bytecode(bytecode);
            if actual_bytecode_hash != hash {
                return Err(EnrichedClientError::custom(
                    "Got invalid base system contract bytecode from main node",
                    "get_bytecode_by_hash",
                )
                .with_arg("hash", &hash)
                .with_arg("actual_bytecode_hash", &actual_bytecode_hash));
            }
        }
        Ok(bytecode)
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        const GENESIS_BLOCK: api::BlockIdVariant =
            api::BlockIdVariant::BlockNumber(api::BlockNumber::Number(U64([0])));

        let code_key = get_code_key(&address);
        let code_hash = self
            .get_storage_at(
                ACCOUNT_CODE_STORAGE_ADDRESS,
                zksync_utils::h256_to_u256(*code_key.key()),
                Some(GENESIS_BLOCK),
            )
            .rpc_context("get_storage_at")
            .with_arg("address", &address)
            .await?;
        self.get_bytecode_by_hash(code_hash)
            .rpc_context("get_bytecode_by_hash")
            .with_arg("code_hash", &code_hash)
            .await
    }

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersion>> {
        self.get_protocol_version(Some(protocol_version as u16))
            .rpc_context("fetch_protocol_version")
            .with_arg("protocol_version", &protocol_version)
            .await
    }

    async fn fetch_genesis_l1_batch_hash(&self) -> EnrichedClientResult<H256> {
        let genesis_l1_batch = self
            .get_l1_batch_details(L1BatchNumber(0))
            .rpc_context("get_l1_batch_details")
            .await?
            .ok_or_else(|| {
                EnrichedClientError::custom(
                    "main node did not return genesis block",
                    "get_l1_batch_details",
                )
            })?;
        genesis_l1_batch.base.root_hash.ok_or_else(|| {
            EnrichedClientError::custom("missing genesis L1 batch hash", "get_l1_batch_details")
        })
    }

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<MiniblockNumber> {
        let number = self
            .get_block_number()
            .rpc_context("get_block_number")
            .await?;
        let number = u32::try_from(number)
            .map_err(|err| EnrichedClientError::custom(err, "u32::try_from"))?;
        Ok(MiniblockNumber(number))
    }

    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<SyncBlock>> {
        self.sync_l2_block(number, with_transactions)
            .rpc_context("fetch_l2_block")
            .with_arg("number", &number)
            .with_arg("with_transactions", &with_transactions)
            .await
    }
}

/// This is a temporary implementation of a cache layer for the main node HTTP requests.
/// It was introduced to quickly develop a way to fetch data from the main node concurrently,
/// while not changing the logic of the fetcher itself.
/// It is intentionally designed in an "easy-to-inject, easy-to-remove" way, so that we can easily
/// switch it to a more performant implementation later.
///
/// The main part of this structure's logic is the ability to concurrently populate the cache
/// of responses and then consume them in a non-concurrent way.
///
/// Note: not every request is guaranteed cached, only the ones that are used to build the action queue.
/// For example, if batch status updater requests a miniblock header long after it was processed by the main
/// fetcher routine, most likely it'll be a cache miss.
#[derive(Debug)]
pub(super) struct CachingMainNodeClient {
    client: Box<dyn MainNodeClient>,
    /// Earliest miniblock number that is not yet cached. Used as a marker to refill the cache.
    next_refill_at: MiniblockNumber,
    blocks: HashMap<MiniblockNumber, SyncBlock>,
}

impl CachingMainNodeClient {
    pub fn new(client: Box<dyn MainNodeClient>) -> Self {
        Self {
            client,
            next_refill_at: MiniblockNumber(0),
            blocks: Default::default(),
        }
    }

    /// Cached version of [`HttpClient::sync_l2_block`].
    pub async fn fetch_l2_block(
        &mut self,
        miniblock: MiniblockNumber,
    ) -> EnrichedClientResult<Option<SyncBlock>> {
        FETCHER_METRICS.cache_total[&CachedMethod::SyncL2Block].inc();
        if let Some(block) = self.blocks.get(&miniblock).cloned() {
            FETCHER_METRICS.cache_hit[&CachedMethod::SyncL2Block].inc();
            return Ok(Some(block));
        }
        let block = self.client.fetch_l2_block(miniblock, true).await?;
        if let Some(block) = block.clone() {
            self.blocks.insert(miniblock, block);
        }
        Ok(block)
    }

    /// Re-export of [`MainNodeClient::fetch_l2_block_number()`]. Added to not expose the internal client.
    pub async fn fetch_l2_block_number(&self) -> EnrichedClientResult<MiniblockNumber> {
        self.client.fetch_l2_block_number().await
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
            .map(|block_number| self.client.fetch_l2_block(block_number, true));

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

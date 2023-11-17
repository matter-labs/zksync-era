use tokio::sync::RwLock;
use zksync_utils::h256_to_u256;

use std::{
    collections::HashMap,
    convert::TryFrom,
    future::Future,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use zksync_config::configs::{api::Web3JsonRpcConfig, chain::NetworkConfig, ContractsConfig};
use zksync_dal::ConnectionPool;
use zksync_types::{
    api::{self, BlockId, BlockNumber, GetLogsFilter},
    block::unpack_block_upgrade_info,
    l2::L2Tx,
    transaction_request::CallRequest,
    AccountTreeId, Address, L1BatchNumber, L1ChainId, L2ChainId, MiniblockNumber, StorageKey, H256,
    SYSTEM_CONTEXT_ADDRESS, U256, U64, VIRTUIAL_BLOCK_UPGRADE_INFO_POSITION,
};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Filter, Log, TypedFilter},
};

use super::metrics::API_METRICS;
use crate::{
    api_server::{
        execution_sandbox::BlockArgs,
        tree::TreeApiHttpClient,
        tx_sender::TxSender,
        web3::{
            backend_jsonrpc::error::internal_error, namespaces::eth::EVENT_TOPIC_NUMBER_LIMIT,
            resolve_block,
        },
    },
    sync_layer::SyncState,
};

/// Configuration values for the API.
/// This structure is detached from `ZkSyncConfig`, since different node types (main, external, etc)
/// may require different configuration layouts.
/// The intention is to only keep the actually used information here.
#[derive(Debug, Clone)]
pub struct InternalApiConfig {
    pub l1_chain_id: L1ChainId,
    pub l2_chain_id: L2ChainId,
    pub max_tx_size: usize,
    pub estimate_gas_scale_factor: f64,
    pub estimate_gas_acceptable_overestimation: u32,
    pub bridge_addresses: api::BridgeAddresses,
    pub diamond_proxy_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub req_entities_limit: usize,
    pub fee_history_limit: u64,
}

impl InternalApiConfig {
    pub fn new(
        eth_config: &NetworkConfig,
        web3_config: &Web3JsonRpcConfig,
        contracts_config: &ContractsConfig,
    ) -> Self {
        Self {
            l1_chain_id: eth_config.network.chain_id(),
            l2_chain_id: eth_config.zksync_network_id,
            max_tx_size: web3_config.max_tx_size,
            estimate_gas_scale_factor: web3_config.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: web3_config
                .estimate_gas_acceptable_overestimation,
            bridge_addresses: api::BridgeAddresses {
                l1_erc20_default_bridge: contracts_config.l1_erc20_bridge_proxy_addr,
                l2_erc20_default_bridge: contracts_config.l2_erc20_bridge_addr,
                l1_weth_bridge: contracts_config.l1_weth_bridge_proxy_addr,
                l2_weth_bridge: contracts_config.l2_weth_bridge_addr,
            },
            diamond_proxy_addr: contracts_config.diamond_proxy_addr,
            l2_testnet_paymaster_addr: contracts_config.l2_testnet_paymaster_addr,
            req_entities_limit: web3_config.req_entities_limit(),
            fee_history_limit: web3_config.fee_history_limit(),
        }
    }
}

/// Thread-safe updatable information about the last sealed miniblock number.
///
/// The information may be temporarily outdated and thus should only be used where this is OK
/// (e.g., for metrics reporting). The value is updated by [`Self::diff()`] and [`Self::diff_with_block_args()`]
/// and on an interval specified when creating an instance.
#[derive(Debug, Clone)]
pub(crate) struct SealedMiniblockNumber(Arc<AtomicU32>);

impl SealedMiniblockNumber {
    /// Creates a handle to the last sealed miniblock number together with a task that will update
    /// it on a schedule.
    pub fn new(
        connection_pool: ConnectionPool,
        update_interval: Duration,
    ) -> (Self, impl Future<Output = ()> + Send) {
        let this = Self(Arc::default());
        let number_updater = this.clone();
        let update_task = async move {
            loop {
                if Arc::strong_count(&number_updater.0) == 1 {
                    // The `sealed_miniblock_number` was dropped; there's no sense continuing updates.
                    tracing::debug!("Stopping latest sealed miniblock updates");
                    break;
                }

                let mut connection = connection_pool.access_storage_tagged("api").await.unwrap();
                let last_sealed_miniblock = connection
                    .blocks_web3_dal()
                    .get_sealed_miniblock_number()
                    .await;
                drop(connection);

                match last_sealed_miniblock {
                    Ok(number) => {
                        number_updater.update(number);
                    }
                    Err(err) => tracing::warn!(
                        "Failed fetching latest sealed miniblock to update the watch channel: {err}"
                    ),
                }
                tokio::time::sleep(update_interval).await;
            }
        };

        (this, update_task)
    }

    /// Potentially updates the last sealed miniblock number by comparing it to the provided
    /// sealed miniblock number (not necessarily the last one).
    ///
    /// Returns the last sealed miniblock number after the update.
    fn update(&self, maybe_newer_miniblock_number: MiniblockNumber) -> MiniblockNumber {
        let prev_value = self
            .0
            .fetch_max(maybe_newer_miniblock_number.0, Ordering::Relaxed);
        MiniblockNumber(prev_value).max(maybe_newer_miniblock_number)
    }

    pub fn diff(&self, miniblock_number: MiniblockNumber) -> u32 {
        let sealed_miniblock_number = self.update(miniblock_number);
        sealed_miniblock_number.0.saturating_sub(miniblock_number.0)
    }

    /// Returns the difference between the latest miniblock number and the resolved miniblock number
    /// from `block_args`.
    pub fn diff_with_block_args(&self, block_args: &BlockArgs) -> u32 {
        // We compute the difference in any case, since it may update the stored value.
        let diff = self.diff(block_args.resolved_block_number());

        if block_args.resolves_to_latest_sealed_miniblock() {
            0 // Overwrite potentially inaccurate value
        } else {
            diff
        }
    }
}

/// Holder for the data required for the API to be functional.
#[derive(Debug)]
pub struct RpcState<E> {
    pub installed_filters: Arc<RwLock<Filters>>,
    pub connection_pool: ConnectionPool,
    pub tree_api: Option<TreeApiHttpClient>,
    pub tx_sender: TxSender<E>,
    pub sync_state: Option<SyncState>,
    pub(super) api_config: InternalApiConfig,
    pub(super) last_sealed_miniblock: SealedMiniblockNumber,
    // The flag that enables redirect of eth get logs implementation to
    // implementation with virtual block translation to miniblocks
    pub logs_translator_enabled: bool,
}

// Custom implementation is required due to generic param:
// Even though it's under `Arc`, compiler doesn't generate the `Clone` implementation unless
// an unnecessary bound is added.
impl<E> Clone for RpcState<E> {
    fn clone(&self) -> Self {
        Self {
            installed_filters: self.installed_filters.clone(),
            connection_pool: self.connection_pool.clone(),
            tx_sender: self.tx_sender.clone(),
            tree_api: self.tree_api.clone(),
            sync_state: self.sync_state.clone(),
            api_config: self.api_config.clone(),
            last_sealed_miniblock: self.last_sealed_miniblock.clone(),
            logs_translator_enabled: self.logs_translator_enabled,
        }
    }
}

impl<E> RpcState<E> {
    pub fn parse_transaction_bytes(&self, bytes: &[u8]) -> Result<(L2Tx, H256), Web3Error> {
        let chain_id = self.api_config.l2_chain_id;
        let (tx_request, hash) = api::TransactionRequest::from_bytes(bytes, chain_id)?;

        Ok((
            L2Tx::from_request(tx_request, self.api_config.max_tx_size)?,
            hash,
        ))
    }

    pub fn u64_to_block_number(n: U64) -> MiniblockNumber {
        if n.as_u64() > u32::MAX as u64 {
            MiniblockNumber(u32::MAX)
        } else {
            MiniblockNumber(n.as_u32())
        }
    }

    pub async fn resolve_filter_block_number(
        &self,
        block_number: Option<api::BlockNumber>,
    ) -> Result<MiniblockNumber, Web3Error> {
        const METHOD_NAME: &str = "resolve_filter_block_number";

        if let Some(api::BlockNumber::Number(number)) = block_number {
            return Ok(Self::u64_to_block_number(number));
        }

        let block_number = block_number.unwrap_or(api::BlockNumber::Latest);
        let block_id = api::BlockId::Number(block_number);
        let mut conn = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        Ok(conn
            .blocks_web3_dal()
            .resolve_block_id(block_id)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .unwrap())
        // ^ `unwrap()` is safe: `resolve_block_id(api::BlockId::Number(_))` can only return `None`
        // if called with an explicit number, and we've handled this case earlier.
    }

    pub async fn resolve_filter_block_range(
        &self,
        filter: &Filter,
    ) -> Result<(MiniblockNumber, MiniblockNumber), Web3Error> {
        let from_block = self.resolve_filter_block_number(filter.from_block).await?;
        let to_block = self.resolve_filter_block_number(filter.to_block).await?;
        Ok((from_block, to_block))
    }

    /// If filter has `block_hash` then it resolves block number by hash and sets it to `from_block` and `to_block`.
    pub async fn resolve_filter_block_hash(&self, filter: &mut Filter) -> Result<(), Web3Error> {
        match (filter.block_hash, filter.from_block, filter.to_block) {
            (Some(block_hash), None, None) => {
                let block_number = self
                    .connection_pool
                    .access_storage_tagged("api")
                    .await
                    .unwrap()
                    .blocks_web3_dal()
                    .resolve_block_id(api::BlockId::Hash(block_hash))
                    .await
                    .map_err(|err| internal_error("resolve_filter_block_hash", err))?
                    .ok_or(Web3Error::NoBlock)?;

                filter.from_block = Some(api::BlockNumber::Number(block_number.0.into()));
                filter.to_block = Some(api::BlockNumber::Number(block_number.0.into()));
                Ok(())
            }
            (Some(_), _, _) => Err(Web3Error::InvalidFilterBlockHash),
            (None, _, _) => Ok(()),
        }
    }

    /// Returns initial `from_block` for filter.
    /// It is equal to max(filter.from_block, PENDING_BLOCK).
    pub async fn get_filter_from_block(
        &self,
        filter: &Filter,
    ) -> Result<MiniblockNumber, Web3Error> {
        const METHOD_NAME: &str = "get_filter_from_block";

        let pending_block = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .expect("Pending block number shouldn't be None");
        let block_number = match filter.from_block {
            Some(api::BlockNumber::Number(number)) => {
                let block_number = Self::u64_to_block_number(number);
                block_number.max(pending_block)
            }
            _ => pending_block,
        };
        Ok(block_number)
    }

    pub(crate) async fn set_nonce_for_call_request(
        &self,
        call_request: &mut CallRequest,
    ) -> Result<(), Web3Error> {
        const METHOD_NAME: &str = "set_nonce_for_call_request";

        if call_request.nonce.is_none() {
            let from = call_request.from.unwrap_or_default();
            let block_id = api::BlockId::Number(api::BlockNumber::Latest);
            let mut connection = self
                .connection_pool
                .access_storage_tagged("api")
                .await
                .unwrap();
            let block_number = resolve_block(&mut connection, block_id, METHOD_NAME).await?;
            let address_historical_nonce = connection
                .storage_web3_dal()
                .get_address_historical_nonce(from, block_number)
                .await
                .map_err(|err| internal_error(METHOD_NAME, err))?;
            call_request.nonce = Some(address_historical_nonce);
        }
        Ok(())
    }

    /// Returns logs for the given filter, taking into account block.number migration with virtual blocks
    pub async fn translate_get_logs(&self, filter: Filter) -> Result<Vec<Log>, Web3Error> {
        const METHOD_NAME: &str = "translate_get_logs";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        // no support for block hash filtering
        if filter.block_hash.is_some() {
            return Err(Web3Error::InvalidFilterBlockHash);
        }

        if let Some(topics) = &filter.topics {
            if topics.len() > EVENT_TOPIC_NUMBER_LIMIT {
                return Err(Web3Error::TooManyTopics);
            }
        }

        let mut conn = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();

        // get virtual block upgrade info
        let upgrade_info = conn
            .storage_dal()
            .get_by_key(&StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                VIRTUIAL_BLOCK_UPGRADE_INFO_POSITION,
            ))
            .await
            .ok_or_else(|| {
                internal_error(
                    METHOD_NAME,
                    "Failed to get virtual block upgrade info from DB".to_string(),
                )
            })?;
        let (virtual_block_start_batch, virtual_block_finish_l2_block) =
            unpack_block_upgrade_info(h256_to_u256(upgrade_info));
        let from_miniblock_number =
            if let Some(BlockNumber::Number(block_number)) = filter.from_block {
                self.resolve_miniblock_from_block(
                    block_number.as_u64(),
                    true,
                    virtual_block_start_batch,
                    virtual_block_finish_l2_block,
                )
                .await?
            } else {
                let block_number = filter.from_block.unwrap_or(BlockNumber::Latest);
                let block_id = BlockId::Number(block_number);
                conn.blocks_web3_dal()
                    .resolve_block_id(block_id)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?
                    .unwrap()
                    .0
            };

        let to_miniblock_number = if let Some(BlockNumber::Number(block_number)) = filter.to_block {
            self.resolve_miniblock_from_block(
                block_number.as_u64(),
                true,
                virtual_block_start_batch,
                virtual_block_finish_l2_block,
            )
            .await?
        } else {
            let block_number = filter.to_block.unwrap_or(BlockNumber::Latest);
            let block_id = BlockId::Number(block_number);
            conn.blocks_web3_dal()
                .resolve_block_id(block_id)
                .await
                .map_err(|err| internal_error(METHOD_NAME, err))?
                .unwrap()
                .0
        };

        // It is considered that all logs of the miniblock where created in the last virtual block
        // of this miniblock. In this case no logs are created.
        // When the given virtual block range is a subrange of some miniblock virtual block range.
        // e.g. given virtual block range is [11, 12] and the miniblock = 5 virtual block range is [10, 14].
        // Then `to_miniblock_number` will be 4 and `from_miniblock_number` will be 5. 4 < 5.
        if to_miniblock_number < from_miniblock_number {
            return Ok(vec![]);
        }

        let block_filter = Filter {
            from_block: Some(from_miniblock_number.into()),
            to_block: Some(to_miniblock_number.into()),
            ..filter.clone()
        };

        let result = self
            .filter_events_changes(
                block_filter,
                MiniblockNumber(from_miniblock_number),
                MiniblockNumber(to_miniblock_number),
            )
            .await;

        method_latency.observe();
        result
    }

    async fn resolve_miniblock_from_block(
        &self,
        block_number: u64,
        is_from: bool,
        virtual_block_start_batch: u64,
        virtual_block_finish_l2_block: u64,
    ) -> Result<u32, Web3Error> {
        const METHOD_NAME: &str = "resolve_miniblock_from_block";

        let mut conn = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();

        if block_number < virtual_block_start_batch {
            let l1_batch = L1BatchNumber(block_number as u32);
            let miniblock_range = conn
                .blocks_web3_dal()
                .get_miniblock_range_of_l1_batch(l1_batch)
                .await
                .map(|minmax| minmax.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
                .map_err(|err| internal_error(METHOD_NAME, err))?;

            match miniblock_range {
                Some((batch_first_miniblock, batch_last_miniblock)) => {
                    if is_from {
                        Ok(batch_first_miniblock.as_u32())
                    } else {
                        Ok(batch_last_miniblock.as_u32())
                    }
                }
                _ => Err(Web3Error::NoBlock),
            }
        } else if virtual_block_finish_l2_block > 0 && block_number >= virtual_block_finish_l2_block
        {
            u32::try_from(block_number).map_err(|_| Web3Error::NoBlock)
        } else {
            // we have to deal with virtual blocks here
            let virtual_block_miniblock = if is_from {
                conn.blocks_web3_dal()
                    .get_miniblock_for_virtual_block_from(virtual_block_start_batch, block_number)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?
            } else {
                conn.blocks_web3_dal()
                    .get_miniblock_for_virtual_block_to(virtual_block_start_batch, block_number)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?
            };
            virtual_block_miniblock.ok_or(Web3Error::NoBlock)
        }
    }

    async fn filter_events_changes(
        &self,
        filter: Filter,
        from_block: MiniblockNumber,
        to_block: MiniblockNumber,
    ) -> Result<Vec<Log>, Web3Error> {
        const METHOD_NAME: &str = "filter_events_changes";

        let addresses: Vec<_> = filter
            .address
            .map_or_else(Vec::default, |address| address.0);
        let topics: Vec<_> = filter
            .topics
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(idx, topics)| topics.map(|topics| (idx as u32 + 1, topics.0)))
            .collect();
        let get_logs_filter = GetLogsFilter {
            from_block,
            to_block: filter.to_block,
            addresses,
            topics,
        };

        let mut storage = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();

        // Check if there is more than one block in range and there are more than `req_entities_limit` logs that satisfies filter.
        // In this case we should return error and suggest requesting logs with smaller block range.
        if from_block != to_block
            && storage
                .events_web3_dal()
                .get_log_block_number(&get_logs_filter, self.api_config.req_entities_limit)
                .await
                .map_err(|err| internal_error(METHOD_NAME, err))?
                .is_some()
        {
            return Err(Web3Error::TooManyLogs(self.api_config.req_entities_limit));
        }

        let logs = storage
            .events_web3_dal()
            .get_logs(get_logs_filter, i32::MAX as usize)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        Ok(logs)
    }
}

/// Contains mapping from index to `Filter` with optional location.
#[derive(Default, Debug, Clone)]
pub struct Filters {
    state: HashMap<U256, TypedFilter>,
    max_cap: usize,
}

impl Filters {
    /// Instantiates `Filters` with given max capacity.
    pub fn new(max_cap: usize) -> Self {
        Self {
            state: Default::default(),
            max_cap,
        }
    }

    /// Adds filter to the state and returns its key.
    pub fn add(&mut self, filter: TypedFilter) -> U256 {
        let idx = loop {
            let val = H256::random().to_fixed_bytes().into();
            if !self.state.contains_key(&val) {
                break val;
            }
        };
        self.state.insert(idx, filter);

        // Check if we reached max capacity
        if self.state.len() > self.max_cap {
            if let Some(first) = self.state.keys().next().cloned() {
                self.remove(first);
            }
        }

        idx
    }

    /// Retrieves filter from the state.
    pub fn get(&self, index: U256) -> Option<&TypedFilter> {
        self.state.get(&index)
    }

    /// Updates filter in the state.
    pub fn update(&mut self, index: U256, new_filter: TypedFilter) -> bool {
        if let Some(typed_filter) = self.state.get_mut(&index) {
            *typed_filter = new_filter;
            true
        } else {
            false
        }
    }

    /// Removes filter from the map.
    pub fn remove(&mut self, index: U256) -> bool {
        self.state.remove(&index).is_some()
    }
}

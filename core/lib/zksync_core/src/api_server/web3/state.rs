use std::{
    future::Future,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use lru::LruCache;
use tokio::sync::Mutex;
use vise::GaugeGuard;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::NetworkConfig, ContractsConfig};
use zksync_dal::ConnectionPool;
use zksync_types::{
    api, l2::L2Tx, transaction_request::CallRequest, Address, L1ChainId, L2ChainId,
    MiniblockNumber, H256, U256, U64,
};
use zksync_web3_decl::{error::Web3Error, types::Filter};

use super::metrics::{FilterType, FILTER_METRICS};
use crate::{
    api_server::{
        execution_sandbox::BlockArgs,
        tree::TreeApiHttpClient,
        tx_sender::TxSender,
        web3::{backend_jsonrpsee::internal_error, resolve_block, TypedFilter},
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
#[derive(Debug, Clone)]
pub struct RpcState {
    pub(crate) installed_filters: Arc<Mutex<Filters>>,
    pub connection_pool: ConnectionPool,
    pub tree_api: Option<TreeApiHttpClient>,
    pub tx_sender: TxSender,
    pub sync_state: Option<SyncState>,
    pub(super) api_config: InternalApiConfig,
    pub(super) last_sealed_miniblock: SealedMiniblockNumber,
}

impl RpcState {
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
}

/// Contains mapping from index to `Filter`x with optional location.
#[derive(Debug)]
pub(crate) struct Filters(LruCache<U256, InstalledFilter>);

#[derive(Debug)]
struct InstalledFilter {
    pub filter: TypedFilter,
    _guard: GaugeGuard,
    created_at: Instant,
    last_request: Instant,
    request_count: usize,
}

impl InstalledFilter {
    pub fn new(filter: TypedFilter) -> Self {
        let guard = FILTER_METRICS.filter_count[&FilterType::from(&filter)].inc_guard(1);
        Self {
            filter,
            _guard: guard,
            created_at: Instant::now(),
            last_request: Instant::now(),
            request_count: 0,
        }
    }

    pub fn update_stats(&mut self) {
        let previous_request_timestamp = self.last_request;
        let now = Instant::now();

        self.last_request = now;
        self.request_count += 1;

        let filter_type = FilterType::from(&self.filter);
        FILTER_METRICS.request_frequency[&filter_type].observe(now - previous_request_timestamp);
    }
}

impl Drop for InstalledFilter {
    fn drop(&mut self) {
        let filter_type = FilterType::from(&self.filter);

        FILTER_METRICS.request_count[&filter_type].observe(self.request_count);
        FILTER_METRICS.filter_lifetime[&filter_type].observe(self.created_at.elapsed());
    }
}

impl Filters {
    /// Instantiates `Filters` with given max capacity.
    pub fn new(max_cap: Option<usize>) -> Self {
        let state = match max_cap {
            Some(max_cap) => {
                LruCache::new(max_cap.try_into().expect("Filter capacity should not be 0"))
            }
            None => LruCache::unbounded(),
        };
        Self(state)
    }

    /// Adds filter to the state and returns its key.
    pub fn add(&mut self, filter: TypedFilter) -> U256 {
        let idx = loop {
            let val = H256::random().to_fixed_bytes().into();
            if !self.0.contains(&val) {
                break val;
            }
        };

        self.0.push(idx, InstalledFilter::new(filter));

        idx
    }

    /// Retrieves filter from the state.
    pub fn get_and_update_stats(&mut self, index: U256) -> Option<TypedFilter> {
        let installed_filter = self.0.get_mut(&index)?;

        installed_filter.update_stats();

        Some(installed_filter.filter.clone())
    }

    /// Updates filter in the state.
    pub fn update(&mut self, index: U256, new_filter: TypedFilter) {
        if let Some(installed_filter) = self.0.get_mut(&index) {
            installed_filter.filter = new_filter;
        }
    }

    /// Removes filter from the map.
    pub fn remove(&mut self, index: U256) -> bool {
        self.0.pop(&index).is_some()
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;

    #[test]
    fn test_filters_functionality() {
        use super::*;

        let mut filters = Filters::new(Some(2));

        let filter1 = TypedFilter::Events(Filter::default(), MiniblockNumber::default());
        let filter2 = TypedFilter::Blocks(MiniblockNumber::default());
        let filter3 = TypedFilter::PendingTransactions(NaiveDateTime::default());

        let idx1 = filters.add(filter1.clone());
        let idx2 = filters.add(filter2);
        let idx3 = filters.add(filter3);

        assert_eq!(filters.0.len(), 2);
        assert!(!filters.0.contains(&idx1));
        assert!(filters.0.contains(&idx2));
        assert!(filters.0.contains(&idx3));

        filters.get_and_update_stats(idx2);

        let idx1 = filters.add(filter1);
        assert_eq!(filters.0.len(), 2);
        assert!(filters.0.contains(&idx1));
        assert!(filters.0.contains(&idx2));
        assert!(!filters.0.contains(&idx3));

        filters.remove(idx1);

        assert_eq!(filters.0.len(), 1);
        assert!(!filters.0.contains(&idx1));
        assert!(filters.0.contains(&idx2));
        assert!(!filters.0.contains(&idx3));
    }
}

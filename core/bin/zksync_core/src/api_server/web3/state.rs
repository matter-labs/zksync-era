use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::NetworkConfig, ContractsConfig};

use crate::api_server::tx_sender::TxSender;
use crate::api_server::web3::{backend_jsonrpc::error::internal_error, resolve_block};
use crate::sync_layer::SyncState;

use zksync_dal::ConnectionPool;

use zksync_types::{
    api, l2::L2Tx, transaction_request::CallRequest, Address, L1ChainId, L2ChainId,
    MiniblockNumber, H256, U256, U64,
};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Filter, TypedFilter},
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
            l2_chain_id: L2ChainId(eth_config.zksync_network_id),
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

/// Holder for the data required for the API to be functional.
#[derive(Debug)]
pub struct RpcState<E> {
    pub installed_filters: Arc<RwLock<Filters>>,
    pub connection_pool: ConnectionPool,
    pub tx_sender: TxSender<E>,
    pub sync_state: Option<SyncState>,
    pub(super) api_config: InternalApiConfig,
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
            sync_state: self.sync_state.clone(),
            api_config: self.api_config.clone(),
        }
    }
}

impl<E> RpcState<E> {
    pub fn parse_transaction_bytes(&self, bytes: &[u8]) -> Result<(L2Tx, H256), Web3Error> {
        let chain_id = self.api_config.l2_chain_id;
        let (tx_request, hash) = api::TransactionRequest::from_bytes(bytes, chain_id.0)?;

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
        let mut conn = self.connection_pool.access_storage_tagged("api").await;
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
            let mut connection = self.connection_pool.access_storage_tagged("api").await;
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

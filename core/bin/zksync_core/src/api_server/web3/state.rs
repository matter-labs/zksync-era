use std::collections::HashMap;
#[cfg(feature = "openzeppelin_tests")]
use std::collections::HashSet;
use std::convert::TryInto;
use std::sync::Arc;

use std::sync::RwLock;

use crate::api_server::tx_sender::TxSender;
use crate::api_server::web3::backend_jsonrpc::error::internal_error;
use crate::sync_layer::SyncState;

use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::{
    api::{self, BlockId, BlockNumber, BridgeAddresses, TransactionRequest},
    l2::L2Tx,
    transaction_request::CallRequest,
    Address, L1ChainId, L2ChainId, MiniblockNumber, H256, U256, U64,
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
    pub bridge_addresses: BridgeAddresses,
    pub diamond_proxy_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub req_entities_limit: usize,
}

impl From<ZkSyncConfig> for InternalApiConfig {
    fn from(config: ZkSyncConfig) -> Self {
        Self {
            l1_chain_id: config.chain.eth.network.chain_id(),
            l2_chain_id: L2ChainId(config.chain.eth.zksync_network_id),
            max_tx_size: config.api.web3_json_rpc.max_tx_size,
            estimate_gas_scale_factor: config.api.web3_json_rpc.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: config
                .api
                .web3_json_rpc
                .estimate_gas_acceptable_overestimation,
            bridge_addresses: BridgeAddresses {
                l1_erc20_default_bridge: config.contracts.l1_erc20_bridge_proxy_addr,
                l2_erc20_default_bridge: config.contracts.l2_erc20_bridge_addr,
            },
            diamond_proxy_addr: config.contracts.diamond_proxy_addr,
            l2_testnet_paymaster_addr: config.contracts.l2_testnet_paymaster_addr,
            req_entities_limit: config.api.web3_json_rpc.req_entities_limit(),
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
    pub accounts: HashMap<Address, PrivateKeySigner>,
    #[cfg(feature = "openzeppelin_tests")]
    pub known_bytecodes: Arc<RwLock<HashSet<Vec<u8>>>>,
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
            accounts: self.accounts.clone(),
            #[cfg(feature = "openzeppelin_tests")]
            known_bytecodes: self.known_bytecodes.clone(),
        }
    }
}

impl<E> RpcState<E> {
    pub fn parse_transaction_bytes(&self, bytes: &[u8]) -> Result<(L2Tx, H256), Web3Error> {
        let chain_id = self.api_config.l2_chain_id;
        let (tx_request, hash) =
            TransactionRequest::from_bytes(bytes, chain_id.0, self.api_config.max_tx_size)?;

        Ok((tx_request.try_into()?, hash))
    }

    pub fn u64_to_block_number(n: U64) -> MiniblockNumber {
        if n.as_u64() > u32::MAX as u64 {
            MiniblockNumber(u32::MAX)
        } else {
            MiniblockNumber(n.as_u32())
        }
    }

    pub fn resolve_filter_block_number(
        &self,
        block_number: Option<api::BlockNumber>,
    ) -> Result<MiniblockNumber, Web3Error> {
        let method_name = "resolve_filter_block_number";
        let block_number = match block_number {
            None => self
                .connection_pool
                .access_storage_blocking()
                .blocks_web3_dal()
                .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
                .map_err(|err| internal_error(method_name, err))?
                .unwrap(),
            Some(api::BlockNumber::Number(number)) => Self::u64_to_block_number(number),
            Some(block_number) => self
                .connection_pool
                .access_storage_blocking()
                .blocks_web3_dal()
                .resolve_block_id(api::BlockId::Number(block_number))
                .map_err(|err| internal_error(method_name, err))?
                .unwrap(),
        };
        Ok(block_number)
    }

    pub fn resolve_filter_block_range(
        &self,
        filter: &Filter,
    ) -> Result<(MiniblockNumber, MiniblockNumber), Web3Error> {
        let from_block = self.resolve_filter_block_number(filter.from_block)?;
        let to_block = self.resolve_filter_block_number(filter.to_block)?;
        Ok((from_block, to_block))
    }

    /// Returns initial `from_block` for filter.
    /// It is equal to max(filter.from_block, PENDING_BLOCK).
    pub fn get_filter_from_block(&self, filter: &Filter) -> Result<MiniblockNumber, Web3Error> {
        let method_name = "get_filter_from_block";
        let pending_block = self
            .connection_pool
            .access_storage_blocking()
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .map_err(|err| internal_error(method_name, err))?
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

    pub(crate) fn set_nonce_for_call_request(
        &self,
        call_request: &mut CallRequest,
    ) -> Result<(), Web3Error> {
        let method_name = "set_nonce_for_call_request";
        if call_request.nonce.is_none() {
            let from = call_request.from.unwrap_or_default();
            let address_historical_nonce = self
                .connection_pool
                .access_storage_blocking()
                .storage_web3_dal()
                .get_address_historical_nonce(from, BlockId::Number(BlockNumber::Latest));

            call_request.nonce = Some(
                address_historical_nonce
                    .unwrap()
                    .map_err(|result| internal_error(method_name, result.to_string()))?,
            );
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

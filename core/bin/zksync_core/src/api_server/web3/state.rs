use std::collections::HashMap;
#[cfg(feature = "openzeppelin_tests")]
use std::collections::HashSet;
use std::convert::TryInto;
use std::sync::Arc;

use std::sync::RwLock;

use crate::api_server::tx_sender::TxSender;
use crate::api_server::web3::backend_jsonrpc::error::internal_error;

use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::api::{self, TransactionRequest};
use zksync_types::{l2::L2Tx, Address, MiniblockNumber, H256, U256, U64};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Filter, TypedFilter},
};

/// Holder for the data required for the API to be functional.
#[derive(Debug, Clone)]
pub struct RpcState {
    pub installed_filters: Arc<RwLock<Filters>>,
    pub connection_pool: ConnectionPool,
    pub tx_sender: TxSender,
    pub req_entities_limit: usize,
    pub config: &'static ZkSyncConfig,
    pub accounts: HashMap<Address, PrivateKeySigner>,
    #[cfg(feature = "openzeppelin_tests")]
    pub known_bytecodes: Arc<RwLock<HashSet<Vec<u8>>>>,
}

impl RpcState {
    pub fn parse_transaction_bytes(&self, bytes: &[u8]) -> Result<(L2Tx, H256), Web3Error> {
        let chain_id = self.config.chain.eth.zksync_network_id;
        let (tx_request, hash) = TransactionRequest::from_bytes(bytes, chain_id)?;

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

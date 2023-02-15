// Built-in uses

// External uses
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;

// Workspace uses
use zksync_types::{
    api::{
        BlockId, BlockIdVariant, BlockNumber, Transaction, TransactionId, TransactionReceipt,
        TransactionVariant,
    },
    transaction_request::CallRequest,
    web3::types::{Index, SyncState},
    Address, Bytes, H256, U256, U64,
};
use zksync_web3_decl::error::Web3Error;
use zksync_web3_decl::types::{Block, Filter, FilterChanges, Log};

// Local uses
use crate::web3::backend_jsonrpc::error::into_jsrpc_error;
use crate::web3::namespaces::EthNamespace;

#[rpc]
pub trait EthNamespaceT {
    #[rpc(name = "eth_blockNumber", returns = "U64")]
    fn get_block_number(&self) -> Result<U64>;

    #[rpc(name = "eth_chainId", returns = "U64")]
    fn chain_id(&self) -> Result<U64>;

    #[rpc(name = "eth_call", returns = "Bytes")]
    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> Result<Bytes>;

    #[rpc(name = "eth_estimateGas", returns = "U256")]
    fn estimate_gas(&self, req: CallRequest, _block: Option<BlockNumber>) -> Result<U256>;

    #[rpc(name = "eth_gasPrice", returns = "U256")]
    fn gas_price(&self) -> Result<U256>;

    #[rpc(name = "eth_newFilter", returns = "U256")]
    fn new_filter(&self, filter: Filter) -> Result<U256>;

    #[rpc(name = "eth_newBlockFilter", returns = "U256")]
    fn new_block_filter(&self) -> Result<U256>;

    #[rpc(name = "eth_uninstallFilter", returns = "U256")]
    fn uninstall_filter(&self, idx: U256) -> Result<bool>;

    #[rpc(name = "eth_newPendingTransactionFilter", returns = "U256")]
    fn new_pending_transaction_filter(&self) -> Result<U256>;

    #[rpc(name = "eth_getLogs", returns = "Vec<Log>")]
    fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

    #[rpc(name = "eth_getFilterLogs", returns = "FilterChanges")]
    fn get_filter_logs(&self, filter_index: U256) -> Result<FilterChanges>;

    #[rpc(name = "eth_getFilterChanges", returns = "FilterChanges")]
    fn get_filter_changes(&self, filter_index: U256) -> Result<FilterChanges>;

    #[rpc(name = "eth_getBalance", returns = "U256")]
    fn get_balance(&self, address: Address, block: Option<BlockIdVariant>) -> Result<U256>;

    #[rpc(
        name = "eth_getBlockByNumber",
        returns = "Option<Block<TransactionVariant>>"
    )]
    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>>;

    #[rpc(
        name = "eth_getBlockByHash",
        returns = "Option<Block<TransactionVariant>>"
    )]
    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>>;

    #[rpc(
        name = "eth_getBlockTransactionCountByNumber",
        returns = "Option<U256>"
    )]
    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<U256>>;

    #[rpc(name = "eth_getBlockTransactionCountByHash", returns = "Option<U256>")]
    fn get_block_transaction_count_by_hash(&self, block_hash: H256) -> Result<Option<U256>>;

    #[rpc(name = "eth_getCode", returns = "Bytes")]
    fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> Result<Bytes>;

    #[rpc(name = "eth_getStorageAt", returns = "H256")]
    fn get_storage(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> Result<H256>;

    #[rpc(name = "eth_getTransactionCount", returns = "U256")]
    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> Result<U256>;

    #[rpc(name = "eth_getTransactionByHash", returns = "Option<Transaction>")]
    fn get_transaction_by_hash(&self, hash: H256) -> Result<Option<Transaction>>;

    #[rpc(
        name = "eth_getTransactionByBlockHashAndIndex",
        returns = "Option<Transaction>"
    )]
    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> Result<Option<Transaction>>;

    #[rpc(
        name = "eth_getTransactionByBlockNumberAndIndex",
        returns = "Option<Transaction>"
    )]
    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> Result<Option<Transaction>>;

    #[rpc(
        name = "eth_getTransactionReceipt",
        returns = "Option<TransactionReceipt>"
    )]
    fn get_transaction_receipt(&self, hash: H256) -> Result<Option<TransactionReceipt>>;

    #[rpc(name = "eth_protocolVersion", returns = "String")]
    fn protocol_version(&self) -> Result<String>;

    #[rpc(name = "eth_sendRawTransaction", returns = "H256")]
    fn send_raw_transaction(&self, tx_bytes: Bytes) -> Result<H256>;

    #[rpc(name = "eth_syncing", returns = "SyncState")]
    fn syncing(&self) -> Result<SyncState>;

    #[rpc(name = "eth_accounts", returns = "Vec<Address>")]
    fn accounts(&self) -> Result<Vec<Address>>;

    #[rpc(name = "eth_coinbase", returns = "Address")]
    fn coinbase(&self) -> Result<Address>;

    #[rpc(name = "eth_getCompilers", returns = "Vec<String>")]
    fn compilers(&self) -> Result<Vec<String>>;

    #[rpc(name = "eth_hashrate", returns = "U256")]
    fn hashrate(&self) -> Result<U256>;

    #[rpc(name = "eth_getUncleCountByBlockHash", returns = "Option<U256>")]
    fn get_uncle_count_by_block_hash(&self, hash: H256) -> Result<Option<U256>>;

    #[rpc(name = "eth_getUncleCountByBlockNumber", returns = "Option<U256>")]
    fn get_uncle_count_by_block_number(&self, number: BlockNumber) -> Result<Option<U256>>;

    #[rpc(name = "eth_mining", returns = "bool")]
    fn mining(&self) -> Result<bool>;

    #[rpc(name = "eth_sendTransaction", returns = "H256")]
    fn send_transaction(
        &self,
        transaction_request: zksync_types::web3::types::TransactionRequest,
    ) -> Result<H256>;
}

impl EthNamespaceT for EthNamespace {
    fn get_block_number(&self) -> Result<U64> {
        self.get_block_number_impl().map_err(into_jsrpc_error)
    }

    fn chain_id(&self) -> Result<U64> {
        Ok(self.chain_id_impl())
    }

    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> Result<Bytes> {
        self.call_impl(req, block.map(Into::into))
            .map_err(into_jsrpc_error)
    }

    fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256> {
        self.estimate_gas_impl(req, block).map_err(into_jsrpc_error)
    }

    fn gas_price(&self) -> Result<U256> {
        self.gas_price_impl().map_err(into_jsrpc_error)
    }

    fn new_filter(&self, filter: Filter) -> Result<U256> {
        self.new_filter_impl(filter).map_err(into_jsrpc_error)
    }

    fn new_block_filter(&self) -> Result<U256> {
        self.new_block_filter_impl().map_err(into_jsrpc_error)
    }

    fn uninstall_filter(&self, idx: U256) -> Result<bool> {
        Ok(self.uninstall_filter_impl(idx))
    }

    fn new_pending_transaction_filter(&self) -> Result<U256> {
        Ok(self.new_pending_transaction_filter_impl())
    }

    fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        self.get_logs_impl(filter).map_err(into_jsrpc_error)
    }

    fn get_filter_logs(&self, filter_index: U256) -> Result<FilterChanges> {
        self.get_filter_logs_impl(filter_index)
            .map_err(into_jsrpc_error)
    }

    fn get_filter_changes(&self, filter_index: U256) -> Result<FilterChanges> {
        self.get_filter_changes_impl(filter_index)
            .map_err(into_jsrpc_error)
    }

    fn get_balance(&self, address: Address, block: Option<BlockIdVariant>) -> Result<U256> {
        self.get_balance_impl(address, block.map(Into::into))
            .map_err(into_jsrpc_error)
    }

    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>> {
        self.get_block_impl(BlockId::Number(block_number), full_transactions)
            .map_err(into_jsrpc_error)
    }

    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>> {
        self.get_block_impl(BlockId::Hash(hash), full_transactions)
            .map_err(into_jsrpc_error)
    }

    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<U256>> {
        self.get_block_transaction_count_impl(BlockId::Number(block_number))
            .map_err(into_jsrpc_error)
    }

    fn get_block_transaction_count_by_hash(&self, block_hash: H256) -> Result<Option<U256>> {
        self.get_block_transaction_count_impl(BlockId::Hash(block_hash))
            .map_err(into_jsrpc_error)
    }

    fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> Result<Bytes> {
        self.get_code_impl(address, block.map(Into::into))
            .map_err(into_jsrpc_error)
    }

    fn get_storage(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> Result<H256> {
        self.get_storage_at_impl(address, idx, block.map(Into::into))
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> Result<U256> {
        self.get_transaction_count_impl(address, block.map(Into::into))
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_by_hash(&self, hash: H256) -> Result<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Hash(hash))
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> Result<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Block(BlockId::Hash(block_hash), index))
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> Result<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Block(BlockId::Number(block_number), index))
            .map_err(into_jsrpc_error)
    }

    fn get_transaction_receipt(&self, hash: H256) -> Result<Option<TransactionReceipt>> {
        self.get_transaction_receipt_impl(hash)
            .map_err(into_jsrpc_error)
    }

    fn protocol_version(&self) -> Result<String> {
        Ok(self.protocol_version())
    }

    fn send_raw_transaction(&self, tx_bytes: Bytes) -> Result<H256> {
        self.send_raw_transaction_impl(tx_bytes)
            .map_err(into_jsrpc_error)
    }

    fn syncing(&self) -> Result<SyncState> {
        Ok(self.syncing_impl())
    }

    fn accounts(&self) -> Result<Vec<Address>> {
        Ok(self.accounts_impl())
    }

    fn coinbase(&self) -> Result<Address> {
        Ok(self.coinbase_impl())
    }

    fn compilers(&self) -> Result<Vec<String>> {
        Ok(self.compilers_impl())
    }

    fn hashrate(&self) -> Result<U256> {
        Ok(self.hashrate_impl())
    }

    fn get_uncle_count_by_block_hash(&self, hash: H256) -> Result<Option<U256>> {
        Ok(self.uncle_count_impl(BlockId::Hash(hash)))
    }

    fn get_uncle_count_by_block_number(&self, number: BlockNumber) -> Result<Option<U256>> {
        Ok(self.uncle_count_impl(BlockId::Number(number)))
    }

    fn mining(&self) -> Result<bool> {
        Ok(self.mining_impl())
    }

    fn send_transaction(
        &self,
        _transaction_request: zksync_types::web3::types::TransactionRequest,
    ) -> Result<H256> {
        #[cfg(feature = "openzeppelin_tests")]
        return self
            .send_transaction_impl(_transaction_request)
            .map_err(into_jsrpc_error);

        #[cfg(not(feature = "openzeppelin_tests"))]
        Err(into_jsrpc_error(Web3Error::NotImplemented))
    }
}

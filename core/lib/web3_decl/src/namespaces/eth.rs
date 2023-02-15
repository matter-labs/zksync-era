// External uses
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

// Workspace uses
use crate::types::{
    Block, Bytes, Filter, FilterChanges, Index, Log, SyncState, TransactionReceipt, U256, U64,
};

use zksync_types::{
    api::Transaction,
    api::{BlockIdVariant, BlockNumber, TransactionVariant},
    transaction_request::CallRequest,
    Address, H256,
};

// Local uses

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "eth")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "eth")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "eth")
)]
pub trait EthNamespace {
    #[method(name = "blockNumber")]
    fn get_block_number(&self) -> RpcResult<U64>;

    #[method(name = "chainId")]
    fn chain_id(&self) -> RpcResult<U64>;

    #[method(name = "call")]
    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> RpcResult<Bytes>;

    #[method(name = "estimateGas")]
    fn estimate_gas(&self, req: CallRequest, _block: Option<BlockNumber>) -> RpcResult<U256>;

    #[method(name = "gasPrice")]
    fn gas_price(&self) -> RpcResult<U256>;

    #[method(name = "newFilter")]
    fn new_filter(&self, filter: Filter) -> RpcResult<U256>;

    #[method(name = "newBlockFilter")]
    fn new_block_filter(&self) -> RpcResult<U256>;

    #[method(name = "uninstallFilter")]
    fn uninstall_filter(&self, idx: U256) -> RpcResult<bool>;

    #[method(name = "newPendingTransactionFilter")]
    fn new_pending_transaction_filter(&self) -> RpcResult<U256>;

    #[method(name = "getLogs")]
    fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;

    #[method(name = "getFilterLogs")]
    fn get_filter_logs(&self, filter_index: U256) -> RpcResult<FilterChanges>;

    #[method(name = "getFilterChanges")]
    fn get_filter_changes(&self, filter_index: U256) -> RpcResult<FilterChanges>;

    #[method(name = "getBalance")]
    fn get_balance(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<U256>;

    #[method(name = "getBlockByNumber")]
    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>>;

    #[method(name = "getBlockByHash")]
    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>>;

    #[method(name = "getBlockTransactionCountByNumber")]
    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> RpcResult<Option<U256>>;

    #[method(name = "getBlockTransactionCountByHash")]
    fn get_block_transaction_count_by_hash(&self, block_hash: H256) -> RpcResult<Option<U256>>;

    #[method(name = "getCode")]
    fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<Bytes>;

    #[method(name = "getStorageAt")]
    fn get_storage_at(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<H256>;

    #[method(name = "getTransactionCount")]
    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<U256>;

    #[method(name = "getTransactionByHash")]
    fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionByBlockHashAndIndex")]
    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionByBlockNumberAndIndex")]
    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionReceipt")]
    fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>>;

    #[method(name = "protocolVersion")]
    fn protocol_version(&self) -> RpcResult<String>;

    #[method(name = "sendRawTransaction")]
    fn send_raw_transaction(&self, tx_bytes: Bytes) -> RpcResult<H256>;

    #[method(name = "syncing")]
    fn syncing(&self) -> RpcResult<SyncState>;

    #[method(name = "accounts")]
    fn accounts(&self) -> RpcResult<Vec<Address>>;

    #[method(name = "coinbase")]
    fn coinbase(&self) -> RpcResult<Address>;

    #[method(name = "getCompilers")]
    fn compilers(&self) -> RpcResult<Vec<String>>;

    #[method(name = "hashrate")]
    fn hashrate(&self) -> RpcResult<U256>;

    #[method(name = "getUncleCountByBlockHash")]
    fn get_uncle_count_by_block_hash(&self, hash: H256) -> RpcResult<Option<U256>>;

    #[method(name = "getUncleCountByBlockNumber")]
    fn get_uncle_count_by_block_number(&self, number: BlockNumber) -> RpcResult<Option<U256>>;

    #[method(name = "mining")]
    fn mining(&self) -> RpcResult<bool>;
}

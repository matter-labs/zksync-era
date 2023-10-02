// External uses
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

// Workspace uses
use crate::types::{
    Block, Bytes, FeeHistory, Filter, FilterChanges, Index, Log, SyncState, TransactionReceipt,
    U256, U64,
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
    async fn get_block_number(&self) -> RpcResult<U64>;

    #[method(name = "chainId")]
    async fn chain_id(&self) -> RpcResult<U64>;

    #[method(name = "call")]
    async fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> RpcResult<Bytes>;

    #[method(name = "estimateGas")]
    async fn estimate_gas(&self, req: CallRequest, _block: Option<BlockNumber>) -> RpcResult<U256>;

    #[method(name = "gasPrice")]
    async fn gas_price(&self) -> RpcResult<U256>;

    #[method(name = "newFilter")]
    async fn new_filter(&self, filter: Filter) -> RpcResult<U256>;

    #[method(name = "newBlockFilter")]
    async fn new_block_filter(&self) -> RpcResult<U256>;

    #[method(name = "uninstallFilter")]
    async fn uninstall_filter(&self, idx: U256) -> RpcResult<bool>;

    #[method(name = "newPendingTransactionFilter")]
    async fn new_pending_transaction_filter(&self) -> RpcResult<U256>;

    #[method(name = "getLogs")]
    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;

    #[method(name = "getFilterLogs")]
    async fn get_filter_logs(&self, filter_index: U256) -> RpcResult<FilterChanges>;

    #[method(name = "getFilterChanges")]
    async fn get_filter_changes(&self, filter_index: U256) -> RpcResult<FilterChanges>;

    #[method(name = "getBalance")]
    async fn get_balance(&self, address: Address, block: Option<BlockIdVariant>)
        -> RpcResult<U256>;

    #[method(name = "getBlockByNumber")]
    async fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>>;

    #[method(name = "getBlockByHash")]
    async fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>>;

    #[method(name = "getBlockTransactionCountByNumber")]
    async fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> RpcResult<Option<U256>>;

    #[method(name = "getBlockTransactionCountByHash")]
    async fn get_block_transaction_count_by_hash(
        &self,
        block_hash: H256,
    ) -> RpcResult<Option<U256>>;

    #[method(name = "getCode")]
    async fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<Bytes>;

    #[method(name = "getStorageAt")]
    async fn get_storage_at(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<H256>;

    #[method(name = "getTransactionCount")]
    async fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<U256>;

    #[method(name = "getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionByBlockHashAndIndex")]
    async fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionByBlockNumberAndIndex")]
    async fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> RpcResult<Option<Transaction>>;

    #[method(name = "getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>>;

    #[method(name = "protocolVersion")]
    async fn protocol_version(&self) -> RpcResult<String>;

    #[method(name = "sendRawTransaction")]
    async fn send_raw_transaction(&self, tx_bytes: Bytes) -> RpcResult<H256>;

    #[method(name = "syncing")]
    async fn syncing(&self) -> RpcResult<SyncState>;

    #[method(name = "accounts")]
    async fn accounts(&self) -> RpcResult<Vec<Address>>;

    #[method(name = "coinbase")]
    async fn coinbase(&self) -> RpcResult<Address>;

    #[method(name = "getCompilers")]
    async fn compilers(&self) -> RpcResult<Vec<String>>;

    #[method(name = "hashrate")]
    async fn hashrate(&self) -> RpcResult<U256>;

    #[method(name = "getUncleCountByBlockHash")]
    async fn get_uncle_count_by_block_hash(&self, hash: H256) -> RpcResult<Option<U256>>;

    #[method(name = "getUncleCountByBlockNumber")]
    async fn get_uncle_count_by_block_number(&self, number: BlockNumber)
        -> RpcResult<Option<U256>>;

    #[method(name = "mining")]
    async fn mining(&self) -> RpcResult<bool>;

    #[method(name = "feeHistory")]
    async fn fee_history(
        &self,
        block_count: U64,
        newest_block: BlockNumber,
        reward_percentiles: Vec<f32>,
    ) -> RpcResult<FeeHistory>;
}

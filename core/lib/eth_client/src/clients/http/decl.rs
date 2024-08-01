use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{BlockDetails, L1BatchDetails},
    web3, Address, L1BatchNumber, L2BlockNumber, H256, U256, U64,
};
use zksync_web3_decl::client::{ForEthereumLikeNetwork, L1, L2};

/// Subset of the L1 `eth` namespace used by the L1 client.
#[rpc(client, namespace = "eth", client_bounds(Self: ForEthereumLikeNetwork))]
pub(super) trait L1EthNamespace {
    #[method(name = "chainId")]
    async fn chain_id(&self) -> RpcResult<U256>;

    #[method(name = "blockNumber")]
    async fn get_block_number(&self) -> RpcResult<U64>;

    // **Important.** Must be called with `full_transactions = false` only.
    #[method(name = "getBlockByNumber")]
    async fn get_block_by_number(
        &self,
        block_number: web3::BlockNumber,
        full_transactions: bool,
    ) -> RpcResult<Option<web3::Block<H256>>>;

    // **Important.** Must be called with `full_transactions = false` only.
    #[method(name = "getBlockByHash")]
    async fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> RpcResult<Option<web3::Block<H256>>>;

    #[method(name = "getTransactionCount")]
    async fn get_transaction_count(
        &self,
        address: Address,
        block: web3::BlockNumber,
    ) -> RpcResult<U256>;

    #[method(name = "gasPrice")]
    async fn gas_price(&self) -> RpcResult<U256>;

    #[method(name = "call")]
    async fn call(&self, req: web3::CallRequest, block: web3::BlockId) -> RpcResult<web3::Bytes>;

    #[method(name = "getBalance")]
    async fn get_balance(&self, address: Address, block: web3::BlockNumber) -> RpcResult<U256>;

    #[method(name = "getLogs")]
    async fn get_logs(&self, filter: web3::Filter) -> RpcResult<Vec<web3::Log>>;

    #[method(name = "sendRawTransaction")]
    async fn send_raw_transaction(&self, tx_bytes: web3::Bytes) -> RpcResult<H256>;

    #[method(name = "feeHistory")]
    async fn fee_history(
        &self,
        block_count: U64,
        newest_block: web3::BlockNumber,
        reward_percentiles: Option<Vec<f32>>,
    ) -> RpcResult<web3::FeeHistory>;

    #[method(name = "getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<web3::Transaction>>;

    #[method(name = "getTransactionReceipt")]
    async fn get_transaction_receipt(
        &self,
        hash: H256,
    ) -> RpcResult<Option<web3::TransactionReceipt>>;
}

/// Subset of the L2 `eth` namespace used by an L2 client. It may have the same
/// methods as the L1 `eth` namespace, but with extended return values.
#[rpc(client, namespace = "eth", client_bounds(Self: ForEthereumLikeNetwork<Net = L2>))]
pub(super) trait L2EthNamespace {
    #[method(name = "feeHistory")]
    async fn fee_history(
        &self,
        block_count: U64,
        newest_block: web3::BlockNumber,
        reward_percentiles: Option<Vec<f32>>,
    ) -> RpcResult<zksync_types::api::FeeHistory>;
}

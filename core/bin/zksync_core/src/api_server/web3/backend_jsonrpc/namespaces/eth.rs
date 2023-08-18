// Built-in uses

// External uses
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;

// Workspace uses
use zksync_types::{
    api::{
        BlockId, BlockIdVariant, BlockNumber, Transaction, TransactionId, TransactionReceipt,
        TransactionVariant,
    },
    transaction_request::CallRequest,
    web3::types::{FeeHistory, Index, SyncState},
    Address, Bytes, H256, U256, U64,
};
use zksync_web3_decl::types::{Block, Filter, FilterChanges, Log};

// Local uses
use crate::web3::namespaces::EthNamespace;
use crate::{l1_gas_price::L1GasPriceProvider, web3::backend_jsonrpc::error::into_jsrpc_error};

#[rpc]
pub trait EthNamespaceT {
    #[rpc(name = "eth_blockNumber")]
    fn get_block_number(&self) -> BoxFuture<Result<U64>>;

    #[rpc(name = "eth_chainId")]
    fn chain_id(&self) -> BoxFuture<Result<U64>>;

    #[rpc(name = "eth_call")]
    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> BoxFuture<Result<Bytes>>;

    #[rpc(name = "eth_estimateGas")]
    fn estimate_gas(
        &self,
        req: CallRequest,
        _block: Option<BlockNumber>,
    ) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_gasPrice")]
    fn gas_price(&self) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_newFilter")]
    fn new_filter(&self, filter: Filter) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_newBlockFilter")]
    fn new_block_filter(&self) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_uninstallFilter")]
    fn uninstall_filter(&self, idx: U256) -> BoxFuture<Result<bool>>;

    #[rpc(name = "eth_newPendingTransactionFilter")]
    fn new_pending_transaction_filter(&self) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_getLogs")]
    fn get_logs(&self, filter: Filter) -> BoxFuture<Result<Vec<Log>>>;

    #[rpc(name = "eth_getFilterLogs")]
    fn get_filter_logs(&self, filter_index: U256) -> BoxFuture<Result<FilterChanges>>;

    #[rpc(name = "eth_getFilterChanges")]
    fn get_filter_changes(&self, filter_index: U256) -> BoxFuture<Result<FilterChanges>>;

    #[rpc(name = "eth_getBalance")]
    fn get_balance(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_getBlockByNumber")]
    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> BoxFuture<Result<Option<Block<TransactionVariant>>>>;

    #[rpc(name = "eth_getBlockByHash")]
    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> BoxFuture<Result<Option<Block<TransactionVariant>>>>;

    #[rpc(name = "eth_getBlockTransactionCountByNumber")]
    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> BoxFuture<Result<Option<U256>>>;

    #[rpc(name = "eth_getBlockTransactionCountByHash")]
    fn get_block_transaction_count_by_hash(
        &self,
        block_hash: H256,
    ) -> BoxFuture<Result<Option<U256>>>;

    #[rpc(name = "eth_getCode")]
    fn get_code(&self, address: Address, block: Option<BlockIdVariant>)
        -> BoxFuture<Result<Bytes>>;

    #[rpc(name = "eth_getStorageAt")]
    fn get_storage(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<H256>>;

    #[rpc(name = "eth_getTransactionCount")]
    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_getTransactionByHash")]
    fn get_transaction_by_hash(&self, hash: H256) -> BoxFuture<Result<Option<Transaction>>>;

    #[rpc(name = "eth_getTransactionByBlockHashAndIndex")]
    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> BoxFuture<Result<Option<Transaction>>>;

    #[rpc(name = "eth_getTransactionByBlockNumberAndIndex")]
    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> BoxFuture<Result<Option<Transaction>>>;

    #[rpc(name = "eth_getTransactionReceipt")]
    fn get_transaction_receipt(&self, hash: H256) -> BoxFuture<Result<Option<TransactionReceipt>>>;

    #[rpc(name = "eth_protocolVersion")]
    fn protocol_version(&self) -> BoxFuture<Result<String>>;

    #[rpc(name = "eth_sendRawTransaction")]
    fn send_raw_transaction(&self, tx_bytes: Bytes) -> BoxFuture<Result<H256>>;

    #[rpc(name = "eth_syncing")]
    fn syncing(&self) -> BoxFuture<Result<SyncState>>;

    #[rpc(name = "eth_accounts")]
    fn accounts(&self) -> BoxFuture<Result<Vec<Address>>>;

    #[rpc(name = "eth_coinbase")]
    fn coinbase(&self) -> BoxFuture<Result<Address>>;

    #[rpc(name = "eth_getCompilers")]
    fn compilers(&self) -> BoxFuture<Result<Vec<String>>>;

    #[rpc(name = "eth_hashrate")]
    fn hashrate(&self) -> BoxFuture<Result<U256>>;

    #[rpc(name = "eth_getUncleCountByBlockHash")]
    fn get_uncle_count_by_block_hash(&self, hash: H256) -> BoxFuture<Result<Option<U256>>>;

    #[rpc(name = "eth_getUncleCountByBlockNumber")]
    fn get_uncle_count_by_block_number(
        &self,
        number: BlockNumber,
    ) -> BoxFuture<Result<Option<U256>>>;

    #[rpc(name = "eth_mining")]
    fn mining(&self) -> BoxFuture<Result<bool>>;

    #[rpc(name = "eth_feeHistory")]
    fn fee_history(
        &self,
        block_count: U64,
        newest_block: BlockNumber,
        reward_percentiles: Vec<f32>,
    ) -> BoxFuture<Result<FeeHistory>>;
}

impl<G: L1GasPriceProvider + Send + Sync + 'static> EthNamespaceT for EthNamespace<G> {
    fn get_block_number(&self) -> BoxFuture<Result<U64>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_block_number_impl()
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn chain_id(&self) -> BoxFuture<Result<U64>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.chain_id_impl()) })
    }

    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> BoxFuture<Result<Bytes>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .call_impl(req, block.map(Into::into))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn estimate_gas(
        &self,
        req: CallRequest,
        block: Option<BlockNumber>,
    ) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .estimate_gas_impl(req, block)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn gas_price(&self) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move { self_.gas_price_impl().map_err(into_jsrpc_error) })
    }

    fn new_filter(&self, filter: Filter) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .new_filter_impl(filter)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn new_block_filter(&self) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .new_block_filter_impl()
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn uninstall_filter(&self, idx: U256) -> BoxFuture<Result<bool>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.uninstall_filter_impl(idx).await) })
    }

    fn new_pending_transaction_filter(&self) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.new_pending_transaction_filter_impl().await) })
    }

    fn get_logs(&self, filter: Filter) -> BoxFuture<Result<Vec<Log>>> {
        let self_ = self.clone();
        Box::pin(async move { self_.get_logs_impl(filter).await.map_err(into_jsrpc_error) })
    }

    fn get_filter_logs(&self, filter_index: U256) -> BoxFuture<Result<FilterChanges>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_filter_logs_impl(filter_index)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_filter_changes(&self, filter_index: U256) -> BoxFuture<Result<FilterChanges>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_filter_changes_impl(filter_index)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_balance(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_balance_impl(address, block.map(Into::into))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> BoxFuture<Result<Option<Block<TransactionVariant>>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_block_impl(BlockId::Number(block_number), full_transactions)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> BoxFuture<Result<Option<Block<TransactionVariant>>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_block_impl(BlockId::Hash(hash), full_transactions)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> BoxFuture<Result<Option<U256>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_block_transaction_count_impl(BlockId::Number(block_number))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_block_transaction_count_by_hash(
        &self,
        block_hash: H256,
    ) -> BoxFuture<Result<Option<U256>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_block_transaction_count_impl(BlockId::Hash(block_hash))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_code(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<Bytes>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_code_impl(address, block.map(Into::into))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_storage(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<H256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_storage_at_impl(address, idx, block.map(Into::into))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_transaction_count_impl(address, block.map(Into::into))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_transaction_by_hash(&self, hash: H256) -> BoxFuture<Result<Option<Transaction>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_transaction_impl(TransactionId::Hash(hash))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> BoxFuture<Result<Option<Transaction>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_transaction_impl(TransactionId::Block(BlockId::Hash(block_hash), index))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> BoxFuture<Result<Option<Transaction>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_transaction_impl(TransactionId::Block(BlockId::Number(block_number), index))
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn get_transaction_receipt(&self, hash: H256) -> BoxFuture<Result<Option<TransactionReceipt>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .get_transaction_receipt_impl(hash)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn protocol_version(&self) -> BoxFuture<Result<String>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.protocol_version()) })
    }

    fn send_raw_transaction(&self, tx_bytes: Bytes) -> BoxFuture<Result<H256>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .send_raw_transaction_impl(tx_bytes)
                .await
                .map_err(into_jsrpc_error)
        })
    }

    fn syncing(&self) -> BoxFuture<Result<SyncState>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.syncing_impl()) })
    }

    fn accounts(&self) -> BoxFuture<Result<Vec<Address>>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.accounts_impl()) })
    }

    fn coinbase(&self) -> BoxFuture<Result<Address>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.coinbase_impl()) })
    }

    fn compilers(&self) -> BoxFuture<Result<Vec<String>>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.compilers_impl()) })
    }

    fn hashrate(&self) -> BoxFuture<Result<U256>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.hashrate_impl()) })
    }

    fn get_uncle_count_by_block_hash(&self, hash: H256) -> BoxFuture<Result<Option<U256>>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.uncle_count_impl(BlockId::Hash(hash))) })
    }

    fn get_uncle_count_by_block_number(
        &self,
        number: BlockNumber,
    ) -> BoxFuture<Result<Option<U256>>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.uncle_count_impl(BlockId::Number(number))) })
    }

    fn mining(&self) -> BoxFuture<Result<bool>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.mining_impl()) })
    }

    fn fee_history(
        &self,
        block_count: U64,
        newest_block: BlockNumber,
        reward_percentiles: Vec<f32>,
    ) -> BoxFuture<Result<FeeHistory>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .fee_history_impl(block_count, newest_block, reward_percentiles)
                .await
                .map_err(into_jsrpc_error)
        })
    }
}

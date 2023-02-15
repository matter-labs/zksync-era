use crate::api_server::web3::namespaces::eth::EthNamespace;

use zksync_types::{
    api::{
        Block, BlockId, BlockIdVariant, BlockNumber, Log, Transaction, TransactionId,
        TransactionReceipt, TransactionVariant,
    },
    transaction_request::CallRequest,
    web3::types::{Index, SyncState},
    Address, Bytes, H256, U256, U64,
};

use zksync_web3_decl::{
    jsonrpsee::{core::RpcResult, types::error::CallError},
    namespaces::eth::EthNamespaceServer,
    types::{Filter, FilterChanges},
};

impl EthNamespaceServer for EthNamespace {
    fn get_block_number(&self) -> RpcResult<U64> {
        self.get_block_number_impl()
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn chain_id(&self) -> RpcResult<U64> {
        Ok(self.chain_id_impl())
    }

    fn call(&self, req: CallRequest, block: Option<BlockIdVariant>) -> RpcResult<Bytes> {
        self.call_impl(req, block.map(Into::into))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> RpcResult<U256> {
        self.estimate_gas_impl(req, block)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn gas_price(&self) -> RpcResult<U256> {
        self.gas_price_impl()
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn new_filter(&self, filter: Filter) -> RpcResult<U256> {
        self.new_filter_impl(filter)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn new_block_filter(&self) -> RpcResult<U256> {
        self.new_block_filter_impl()
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn uninstall_filter(&self, idx: U256) -> RpcResult<bool> {
        Ok(self.uninstall_filter_impl(idx))
    }

    fn new_pending_transaction_filter(&self) -> RpcResult<U256> {
        Ok(self.new_pending_transaction_filter_impl())
    }

    fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        self.get_logs_impl(filter)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_filter_logs(&self, filter_index: U256) -> RpcResult<FilterChanges> {
        self.get_filter_logs_impl(filter_index)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_filter_changes(&self, filter_index: U256) -> RpcResult<FilterChanges> {
        self.get_filter_changes_impl(filter_index)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_balance(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<U256> {
        self.get_balance_impl(address, block.map(Into::into))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>> {
        self.get_block_impl(BlockId::Number(block_number), full_transactions)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>> {
        self.get_block_impl(BlockId::Hash(hash), full_transactions)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> RpcResult<Option<U256>> {
        self.get_block_transaction_count_impl(BlockId::Number(block_number))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_block_transaction_count_by_hash(&self, block_hash: H256) -> RpcResult<Option<U256>> {
        self.get_block_transaction_count_impl(BlockId::Hash(block_hash))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<Bytes> {
        self.get_code_impl(address, block.map(Into::into))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_storage_at(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<H256> {
        self.get_storage_at_impl(address, idx, block.map(Into::into))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<U256> {
        self.get_transaction_count_impl(address, block.map(Into::into))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Hash(hash))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> RpcResult<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Block(BlockId::Hash(block_hash), index))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> RpcResult<Option<Transaction>> {
        self.get_transaction_impl(TransactionId::Block(BlockId::Number(block_number), index))
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>> {
        self.get_transaction_receipt_impl(hash)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn protocol_version(&self) -> RpcResult<String> {
        Ok(self.protocol_version())
    }

    fn send_raw_transaction(&self, tx_bytes: Bytes) -> RpcResult<H256> {
        self.send_raw_transaction_impl(tx_bytes)
            .map_err(|err| CallError::from_std_error(err).into())
    }

    fn syncing(&self) -> RpcResult<SyncState> {
        Ok(self.syncing_impl())
    }

    fn accounts(&self) -> RpcResult<Vec<Address>> {
        Ok(self.accounts_impl())
    }

    fn coinbase(&self) -> RpcResult<Address> {
        Ok(self.coinbase_impl())
    }

    fn compilers(&self) -> RpcResult<Vec<String>> {
        Ok(self.compilers_impl())
    }

    fn hashrate(&self) -> RpcResult<U256> {
        Ok(self.hashrate_impl())
    }

    fn get_uncle_count_by_block_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
        Ok(self.uncle_count_impl(BlockId::Hash(hash)))
    }

    fn get_uncle_count_by_block_number(&self, number: BlockNumber) -> RpcResult<Option<U256>> {
        Ok(self.uncle_count_impl(BlockId::Number(number)))
    }

    fn mining(&self) -> RpcResult<bool> {
        Ok(self.mining_impl())
    }
}

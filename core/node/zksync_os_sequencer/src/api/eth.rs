use async_trait::async_trait;
use zk_os_forward_system::run::PreimageSource;
use zksync_web3_decl::jsonrpsee::core::RpcResult;
use zksync_web3_decl::types::{Block, Bytes, Filter, FilterChanges, Index, Log, SyncState, TransactionReceipt, U256, U64, U64Number};
use crate::{CHAIN_ID, MAX_TX_SIZE};
use crate::storage::in_memory_state::{InMemoryStorage};
use zksync_zkos_vm_runner::zkos_conversions::{h256_to_bytes32, tx_abi_encode};

use zksync_web3_decl::{namespaces::EthNamespaceServer};

use zksync_types::{api::{
    state_override::StateOverride, BlockId, BlockIdVariant, BlockNumber, FeeHistory, Transaction,
    TransactionVariant,
}, transaction_request::CallRequest, Address, H256, api, L2ChainId};
use zksync_types::l2::L2Tx;
use zksync_web3_decl::jsonrpsee::types::{ErrorCode, ErrorObject};
use zksync_web3_decl::jsonrpsee::types::error::INTERNAL_ERROR_CODE;
use crate::api::resolve_block_id;
use crate::mempool::Mempool;
use crate::storage::StateHandle;

pub(crate) struct EthNamespace {
    state_handle: StateHandle,
    mempool: Mempool,
}
impl EthNamespace {
    pub fn new(
        state_handle: StateHandle,
        mempool: Mempool,
    ) -> Self {
        Self { state_handle, mempool }
    }
}

//
#[async_trait]
impl EthNamespaceServer for EthNamespace {
    async fn get_block_number(&self) -> RpcResult<U64> {
        // todo: really add plus one?
        let res = self.state_handle.last_canonized_block_number() + 1;
        tracing::debug!("get_block_number: res: {:?}", res);
        Ok(res.into())
    }

    async fn chain_id(&self) -> RpcResult<U64> {
        Ok(CHAIN_ID.into())
    }

    async fn call(
        &self,
        req: CallRequest,
        block: Option<BlockIdVariant>,
        state_override: Option<StateOverride>,
    ) -> RpcResult<Bytes> {
        if state_override.is_some() {
            Err(ErrorObject::borrowed(
                INTERNAL_ERROR_CODE,
                "State override is not supported",
                None,
            ))?;
        }
        unimplemented!()
    }

    async fn estimate_gas(
        &self,
        req: CallRequest,
        block: Option<BlockNumber>,
        state_override: Option<StateOverride>,
    ) -> RpcResult<U256> {
        unimplemented!()
    }

    async fn gas_price(&self) -> RpcResult<U256> {
        Ok(U256::from(1000).into())
    }

    async fn new_filter(&self, filter: Filter) -> RpcResult<U256> {
        unimplemented!()
    }

    async fn new_block_filter(&self) -> RpcResult<U256> {
        unimplemented!()
    }

    async fn uninstall_filter(&self, idx: U256) -> RpcResult<bool> {
        unimplemented!()
    }

    async fn new_pending_transaction_filter(&self) -> RpcResult<U256> {
        unimplemented!()
    }

    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        unimplemented!()
    }

    async fn get_filter_logs(&self, filter_index: U256) -> RpcResult<FilterChanges> {
        unimplemented!()
    }

    async fn get_filter_changes(&self, filter_index: U256) -> RpcResult<FilterChanges> {
        unimplemented!()
    }

    async fn get_balance(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<U256> {
        let block_number = resolve_block_id(block, self.state_handle.clone());
        let balance = self.state_handle
            .0
            .account_property_history
            .get(block_number, &address)
            .map(|props| U256::from_big_endian(&props.balance.to_be_bytes::<32>()))
            .unwrap_or(U256::zero());

        tracing::debug!("get_balance: address: {:?}, block: {:?}, balance: {:?}",
            address, block, balance);

        Ok(balance.into())
    }

    async fn get_block_by_number(
        &self,
        block_number: BlockNumber,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>> {
        unimplemented!()
    }

    async fn get_block_by_hash(
        &self,
        hash: H256,
        full_transactions: bool,
    ) -> RpcResult<Option<Block<TransactionVariant>>> {
        unimplemented!()
    }

    async fn get_block_transaction_count_by_number(
        &self,
        block_number: BlockNumber,
    ) -> RpcResult<Option<U256>> {
        unimplemented!()
    }

    async fn get_block_receipts(
        &self,
        block_id: BlockId,
    ) -> RpcResult<Option<Vec<TransactionReceipt>>> {
        unimplemented!()
    }

    async fn get_block_transaction_count_by_hash(
        &self,
        block_hash: H256,
    ) -> RpcResult<Option<U256>> {
        unimplemented!()
    }

    async fn get_code(&self, address: Address, block: Option<BlockIdVariant>) -> RpcResult<Bytes> {
        unimplemented!()
    }

    async fn get_storage_at(
        &self,
        address: Address,
        idx: U256,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<H256> {
        unimplemented!()
    }

    async fn get_transaction_count(
        &self,
        address: Address,
        block: Option<BlockIdVariant>,
    ) -> RpcResult<U256> {
        let block_number = resolve_block_id(block, self.state_handle.clone());
        let nonce = self.state_handle
            .0
            .account_property_history
            .get(block_number, &address)
            .map(|props| props.nonce)
            .unwrap_or(0);

        Ok(nonce.into())
    }

    async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
        let res = self.state_handle.0.in_memory_tx_receipts.get(&h256_to_bytes32(hash));
        tracing::debug!("get_transaction_by_hash: hash: {:?}, res: {:?}", hash, res);
        Ok(res.map(|data| data.transaction))
    }

    async fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: Index,
    ) -> RpcResult<Option<Transaction>> {
        unimplemented!()
    }

    async fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumber,
        index: Index,
    ) -> RpcResult<Option<Transaction>> {
        unimplemented!()
    }

    async fn get_transaction_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>> {
        let res = self.state_handle.0.in_memory_tx_receipts.get(&h256_to_bytes32(hash));
        tracing::debug!("get_transaction_by_hash: hash: {:?}, res: {:?}", hash, res);
        Ok(res.map(|data| data.receipt))
    }

    async fn protocol_version(&self) -> RpcResult<String> {
        unimplemented!()
    }

    async fn send_raw_transaction(&self, tx_bytes: Bytes) -> RpcResult<H256> {
        let (tx_request, hash) =
            api::TransactionRequest::from_bytes(&tx_bytes.0, L2ChainId::new(CHAIN_ID).unwrap())
                .map_err(|_| ErrorObject::owned(
                    ErrorCode::InternalError.code(),
                    "Invalid transaction bytes",
                    None::<()>,
                ))?;
        tracing::debug!("Sending TransactionRequest: {:?}, hash: {}", tx_request, hash);

        let mut l2_tx = L2Tx::from_request(tx_request, MAX_TX_SIZE, true)
            .map_err(|_| ErrorObject::owned(
                ErrorCode::InternalError.code(),
                "Failed to convert transaction request to L2Tx",
                None::<()>,
            ))?;

        l2_tx.set_input(tx_bytes.0, hash);
        tracing::debug!("Converted L2Tx: {:?}", l2_tx);

        let tx = zksync_types::Transaction::from(l2_tx);
        self.mempool.insert(tx);

        Ok(hash)
    }

    async fn syncing(&self) -> RpcResult<SyncState> {
        unimplemented!()
    }

    async fn accounts(&self) -> RpcResult<Vec<Address>> {
        unimplemented!()
    }

    async fn coinbase(&self) -> RpcResult<Address> {
        unimplemented!()
    }

    async fn compilers(&self) -> RpcResult<Vec<String>> {
        unimplemented!()
    }

    async fn hashrate(&self) -> RpcResult<U256> {
        unimplemented!()
    }

    async fn get_uncle_count_by_block_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
        unimplemented!()
    }

    async fn get_uncle_count_by_block_number(
        &self,
        number: BlockNumber,
    ) -> RpcResult<Option<U256>> {
        unimplemented!()
    }

    async fn mining(&self) -> RpcResult<bool> {
        unimplemented!()
    }

    async fn fee_history(
        &self,
        block_count: U64Number,
        newest_block: BlockNumber,
        reward_percentiles: Option<Vec<f32>>,
    ) -> RpcResult<FeeHistory> {
        unimplemented!()
    }

    async fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
        unimplemented!()
    }
}
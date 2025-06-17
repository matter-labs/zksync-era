use anyhow::Context;
use async_trait::async_trait;
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties;
use zk_os_forward_system::run::{ExecutionResult, PreimageSource};
use zksync_web3_decl::jsonrpsee::core::RpcResult;
use zksync_web3_decl::types::{Block, Bytes, Filter, FilterChanges, Index, Log, SyncState, TransactionReceipt, U256, U64, U64Number};
use crate::{CHAIN_ID, DEFAULT_ETH_CALL_GAS, MAX_TX_SIZE};
use crate::storage::storage_map::{StorageMap};
use zksync_zkos_vm_runner::zkos_conversions::{h256_to_bytes32, ruint_u256_to_api_u256, tx_abi_encode};

use zksync_web3_decl::{namespaces::EthNamespaceServer};

use zksync_types::{api::{
    state_override::StateOverride, BlockId, BlockIdVariant, BlockNumber, FeeHistory, Transaction,
    TransactionVariant,
}, transaction_request::CallRequest, Address, H256, api, L2ChainId, PackedEthSignature};
use zksync_types::l2::L2Tx;
use zksync_web3_decl::jsonrpsee::types::{ErrorCode, ErrorObject};
use zksync_web3_decl::jsonrpsee::types::error::INTERNAL_ERROR_CODE;
use crate::api::api_tx_operations::{validate_tx_nonce, validate_tx_sender_balance};
use crate::api::resolve_block_id;
use crate::execution::sandbox::execute;
use crate::mempool::Mempool;
use crate::storage::block_replay_storage::BlockReplayStorage;
use crate::storage::StateHandle;
use crate::api::metrics::{API_METRICS};


pub(crate) struct EthNamespace {
    state_handle: StateHandle,
    mempool: Mempool,
    block_replay_storage: BlockReplayStorage,
}

impl EthNamespace {
    pub fn map_err(&self, err: anyhow::Error) -> ErrorObject<'static> {
        tracing::warn!("Error in EthNamespace: {}", err);
        ErrorObject::owned(
            INTERNAL_ERROR_CODE,
            err.to_string(),
            None::<()>,
        )
    }

    pub fn new(
        state_handle: StateHandle,
        mempool: Mempool,
        block_replay_storage: BlockReplayStorage,
    ) -> Self {
        Self { state_handle, mempool, block_replay_storage }
    }

    pub fn send_raw_transaction_impl(
        &self,
        tx_bytes: Bytes,
    ) -> anyhow::Result<H256> {

        // todo: don't use Transaction types from Types
        let (tx_request, hash) =
            api::TransactionRequest::from_bytes(&tx_bytes.0, L2ChainId::new(CHAIN_ID).unwrap())?;
        let mut l2_tx = L2Tx::from_request(tx_request, MAX_TX_SIZE, true)?;
        l2_tx.set_input(tx_bytes.0, hash);

        let sender_account_properties = self
            .state_handle.0.account_property_history.get_latest(&l2_tx.initiator_account());

        // tracing::info!(
        //     "Processing transaction: {:?}, sender properties: {:?}",
        //     l2_tx,
        //     sender_account_properties
        // );

        validate_tx_sender_balance(
            &l2_tx,
            &sender_account_properties,
        )?;

        validate_tx_nonce(
            &l2_tx,
            &sender_account_properties,
        )?;

        let block_number = self.state_handle.last_canonized_block_number() + 1;
        let block_context = self
            .block_replay_storage
            .get_context(block_number - 1)
            .context("Failed to get block context")?;

        let storage_view = self.state_handle.view_at(block_number)?;
        //
        // let res = execute(
        //     l2_tx.clone(),
        //     block_context,
        //     storage_view,
        // )?;

        self.mempool.insert(l2_tx.into());

        Ok(hash)
    }

    pub fn call_impl(
        &self,
        mut req: CallRequest,
        block: Option<BlockIdVariant>,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<Bytes> {
        anyhow::ensure!(state_override.is_none());

        if req.gas.is_none() {
            req.gas = Some(DEFAULT_ETH_CALL_GAS.into());
        }

        // todo: doing `+ 1` to make sure eth_calls are done on top of the current chain
        // consider legacy logic - perhaps differentiate Latest/Commiteetd etc
        let block_number = resolve_block_id(block, self.state_handle.clone()) + 1;
        tracing::info!("block {:?} resolved to: {:?}", block, block_number);

        let mut tx = L2Tx::from_request(
            req.clone().into(),
            MAX_TX_SIZE,
            true,
        )?;

        // otherwise it's not parsed properly in VM
        if tx.common_data.signature.is_empty() {
            tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
        }

        // using previous block context
        let block_context = self
            .block_replay_storage
            .get_replay_record(block_number - 1)
            .context("Failed to get block context")?
            .context;

        let storage_view = self.state_handle.view_at(block_number)?;

        let res = execute(
            tx,
            block_context,
            storage_view,
        )?;

        Ok(res.as_returned_bytes().into())
    }
}

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
        let latency = API_METRICS.response_time[&"call"].start();
        let r = self.call_impl(
            req,
            block,
            state_override,
        )
            .map_err(|err| self.map_err(err));
        latency.observe();
        r
    }

    async fn estimate_gas(
        &self,
        req: CallRequest,
        block: Option<BlockNumber>,
        state_override: Option<StateOverride>,
    ) -> RpcResult<U256> {
        let latency = API_METRICS.response_time[&"estimate_gas"].start();
        latency.observe();
        Ok(U256::from("1000000"))
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
        //todo: this +1 will kill me
        let block_number = resolve_block_id(block, self.state_handle.clone()) + 1;
        let balance = self.state_handle
            .0
            .account_property_history
            .get(block_number, &address)
            .map(|props| ruint_u256_to_api_u256(props.balance))
            .unwrap_or(U256::zero());

        tracing::info!("get_balance: address: {:?}, block: {:?}, balance: {:?}",
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
        // returning nonce from ethemeral txs

        let nonce_from_pending = self
            .state_handle.0.account_property_history.get_latest(&address).map(|prop| prop.nonce).unwrap_or(0);
        let block_number = resolve_block_id(block, self.state_handle.clone()) + 1;

        let nonce_from_canonized = self.state_handle
            .0
            .account_property_history
            .get(block_number, &address)
            .map(|props| props.nonce)
            .unwrap_or(0);

        tracing::info!(
            "get_transaction_count: address: {:?}, block: {:?}, nonce_from_pending: {:?}, nonce_from_canonized: {:?}",
            address, block, nonce_from_pending, nonce_from_canonized
        );

        Ok(nonce_from_pending.into())
    }

    async fn get_transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
        let res = self.state_handle.0.in_memory_tx_receipts.get(&h256_to_bytes32(hash));
        tracing::info!("get_transaction_by_hash: hash: {:?}, res: {:?}", hash, res);
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
        let latency = API_METRICS.response_time[&"send_raw_transaction"].start();

        let r = self.send_raw_transaction_impl(tx_bytes)
            .map_err(|err| self.map_err(err));
        latency.observe();

        r
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
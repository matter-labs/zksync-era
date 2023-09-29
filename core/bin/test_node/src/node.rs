//! In-memory node, that supports forking other networks.
use crate::{
    fork::{ForkDetails, ForkStorage},
    utils::IntoBoxedFuture,
};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use once_cell::sync::Lazy;

use zksync_basic_types::{web3::types::FeeHistory, AccountTreeId, Bytes, H160, H256, U256, U64};
use zksync_contracts::BaseSystemContracts;
use zksync_core::api_server::web3::backend_jsonrpc::namespaces::eth::EthNamespaceT;
use zksync_state::{ReadStorage, StorageView, WriteStorage};
use zksync_types::{
    api::{BlockNumber, TransactionReceipt, TransactionVariant},
    get_code_key, get_nonce_key,
    l2::L2Tx,
    transaction_request::TransactionRequest,
    tx::tx_execution_info::TxExecutionStatus,
    utils::{storage_key_for_eth_balance, storage_key_for_standard_token_balance},
    StorageKey, StorageLogQueryType, Transaction, ACCOUNT_CODE_STORAGE_ADDRESS,
    L2_ETH_TOKEN_ADDRESS,
};
use zksync_utils::{h256_to_account_address, h256_to_u256, h256_to_u64, u256_to_h256};

use vm::{
    utils::{create_test_block_params, BASE_SYSTEM_CONTRACTS, BLOCK_GAS_LIMIT, ETH_CALL_GAS_LIMIT},
    vm::VmTxExecutionResult,
    vm_with_bootloader::{
        init_vm_inner, push_transaction_to_bootloader_memory, BlockContextMode, BootloaderJobType,
        TxExecutionMode,
    },
    HistoryEnabled, OracleTools,
};
use zksync_web3_decl::types::{Filter, FilterChanges};

pub const MAX_TX_SIZE: usize = 1000000;
// Timestamp of the first block (if not running in fork mode).
pub const NON_FORK_FIRST_BLOCK_TIMESTAMP: u64 = 1000;
/// Network ID we use for the test node.
pub const TEST_NODE_NETWORK_ID: u16 = 270;

/// Basic information about the generated block (which is block l1 batch and miniblock).
/// Currently, this test node supports exactly one transaction per block.
pub struct BlockInfo {
    pub batch_number: u32,
    pub block_timestamp: u64,
    /// Transaction included in this block.
    pub tx_hash: H256,
}

/// Information about the executed transaction.
pub struct TxExecutionInfo {
    pub tx: L2Tx,
    // Batch number where transaction was executed.
    pub batch_number: u32,
    pub miniblock_number: u64,
    pub result: VmTxExecutionResult,
}

/// Helper struct for InMemoryNode.
pub struct InMemoryNodeInner {
    /// Timestamp, batch number and miniblock number that will be used by the next block.
    pub current_timestamp: u64,
    pub current_batch: u32,
    pub current_miniblock: u64,
    // Map from transaction to details about the exeuction
    pub tx_results: HashMap<H256, TxExecutionInfo>,
    // Map from batch number to information about the block.
    pub blocks: HashMap<u32, BlockInfo>,
    // Underlying storage
    pub fork_storage: ForkStorage,
}

fn not_implemented<T: Send + 'static>() -> jsonrpc_core::BoxFuture<Result<T, jsonrpc_core::Error>> {
    Err(jsonrpc_core::Error::method_not_found()).into_boxed_future()
}

/// In-memory node, that can be used for local & unit testing.
/// It also supports the option of forking testnet/mainnet.
/// All contents are removed when object is destroyed.
pub struct InMemoryNode {
    inner: Arc<RwLock<InMemoryNodeInner>>,
}

pub static PLAYGROUND_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::playground);

fn contract_address_from_tx_result(execution_result: &VmTxExecutionResult) -> Option<H160> {
    for query in execution_result.result.logs.storage_logs.iter().rev() {
        if query.log_type == StorageLogQueryType::InitialWrite
            && query.log_query.address == ACCOUNT_CODE_STORAGE_ADDRESS
        {
            return Some(h256_to_account_address(&u256_to_h256(query.log_query.key)));
        }
    }
    None
}

impl InMemoryNode {
    pub fn new(fork: Option<ForkDetails>) -> Self {
        InMemoryNode {
            inner: Arc::new(RwLock::new(InMemoryNodeInner {
                current_timestamp: fork
                    .as_ref()
                    .map(|f| f.block_timestamp + 1)
                    .unwrap_or(NON_FORK_FIRST_BLOCK_TIMESTAMP),
                current_batch: fork.as_ref().map(|f| f.l1_block.0 + 1).unwrap_or(1),
                current_miniblock: fork.as_ref().map(|f| f.l2_miniblock + 1).unwrap_or(1),
                tx_results: Default::default(),
                blocks: Default::default(),
                fork_storage: ForkStorage::new(fork),
            })),
        }
    }

    /// Applies multiple transactions - but still one per L1 batch.
    pub fn apply_txs(&self, txs: Vec<L2Tx>) {
        println!("Running {:?} transactions (one per batch)", txs.len());

        for tx in txs {
            println!("Executing {:?}", tx.hash());
            self.run_l2_tx(tx, TxExecutionMode::VerifyExecute);
        }
    }

    /// Adds a lot of tokens to a given account.
    pub fn set_rich_account(&self, address: H160) {
        let key = storage_key_for_eth_balance(&address);
        let mut inner = self.inner.write().unwrap();
        let keys = {
            let mut storage_view = StorageView::new(&inner.fork_storage);
            storage_view.set_value(key, u256_to_h256(U256::from(10u64.pow(19))));
            storage_view.modified_storage_keys().clone()
        };

        for (key, value) in keys.iter() {
            inner.fork_storage.set_value(*key, *value);
        }
    }

    /// Runs L2 'eth call' method - that doesn't commit to a block.
    fn run_l2_call(&self, l2_tx: L2Tx) -> Vec<u8> {
        let execution_mode = TxExecutionMode::EthCall {
            missed_storage_invocation_limit: 1000000,
        };
        let (mut block_context, block_properties) = create_test_block_params();

        let inner = self.inner.write().unwrap();

        let mut storage_view = StorageView::new(&inner.fork_storage);

        let mut oracle_tools = OracleTools::new(&mut storage_view, HistoryEnabled);

        let bootloader_code = &PLAYGROUND_SYSTEM_CONTRACTS;
        block_context.block_number = inner.current_batch;
        block_context.block_timestamp = inner.current_timestamp;

        // init vm
        let mut vm = init_vm_inner(
            &mut oracle_tools,
            BlockContextMode::NewBlock(block_context.into(), Default::default()),
            &block_properties,
            BLOCK_GAS_LIMIT,
            bootloader_code,
            execution_mode,
        );

        let tx: Transaction = l2_tx.into();

        push_transaction_to_bootloader_memory(&mut vm, &tx, execution_mode, None);

        let vm_block_result =
            vm.execute_till_block_end_with_call_tracer(BootloaderJobType::TransactionExecution);

        match vm_block_result.full_result.revert_reason {
            Some(result) => result.original_data,
            None => vm_block_result
                .full_result
                .return_data
                .into_iter()
                .flat_map(|val| {
                    let bytes: [u8; 32] = val.into();
                    bytes.to_vec()
                })
                .collect::<Vec<_>>(),
        }
    }

    fn run_l2_tx_inner(
        &self,
        l2_tx: L2Tx,
        execution_mode: TxExecutionMode,
    ) -> (
        HashMap<StorageKey, H256>,
        VmTxExecutionResult,
        BlockInfo,
        HashMap<U256, Vec<U256>>,
    ) {
        let (mut block_context, block_properties) = create_test_block_params();

        let inner = self.inner.write().unwrap();

        let mut storage_view = StorageView::new(&inner.fork_storage);

        let mut oracle_tools = OracleTools::new(&mut storage_view, HistoryEnabled);

        let bootloader_code = if execution_mode == TxExecutionMode::VerifyExecute {
            &BASE_SYSTEM_CONTRACTS
        } else {
            &PLAYGROUND_SYSTEM_CONTRACTS
        };

        block_context.block_number = inner.current_batch;
        block_context.block_timestamp = inner.current_timestamp;
        let block = BlockInfo {
            batch_number: block_context.block_number,
            block_timestamp: block_context.block_timestamp,
            tx_hash: l2_tx.hash(),
        };

        // init vm
        let mut vm = init_vm_inner(
            &mut oracle_tools,
            BlockContextMode::NewBlock(block_context.into(), Default::default()),
            &block_properties,
            BLOCK_GAS_LIMIT,
            bootloader_code,
            execution_mode,
        );

        let tx: Transaction = l2_tx.into();

        push_transaction_to_bootloader_memory(&mut vm, &tx, execution_mode, None);

        let tx_result = vm.execute_next_tx(u32::MAX, true).unwrap();

        println!(
            "Tx Execution results: {:?} {:?}",
            tx_result.status, tx_result.result.revert_reason
        );

        vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing);

        let bytecodes = vm
            .state
            .decommittment_processor
            .known_bytecodes
            .inner()
            .clone();

        let modified_keys = storage_view.modified_storage_keys().clone();
        (modified_keys, tx_result, block, bytecodes)
    }

    /// Runs L2 transaction and commits it to a new block.
    fn run_l2_tx(&self, l2_tx: L2Tx, execution_mode: TxExecutionMode) {
        let tx_hash = l2_tx.hash();

        let (keys, result, block, bytecodes) = self.run_l2_tx_inner(l2_tx.clone(), execution_mode);

        // Write all the mutated keys (storage slots).
        let mut inner = self.inner.write().unwrap();
        for (key, value) in keys.iter() {
            inner.fork_storage.set_value(*key, *value);
        }

        // Write all the factory deps.
        for (hash, code) in bytecodes.iter() {
            inner.fork_storage.store_factory_dep(
                u256_to_h256(*hash),
                code.iter()
                    .flat_map(|entry| {
                        let mut bytes = vec![0u8; 32];
                        entry.to_big_endian(&mut bytes);
                        bytes.to_vec()
                    })
                    .collect(),
            )
        }
        let current_miniblock = inner.current_miniblock;
        inner.tx_results.insert(
            tx_hash,
            TxExecutionInfo {
                tx: l2_tx,
                batch_number: block.batch_number,
                miniblock_number: current_miniblock,
                result,
            },
        );
        inner.blocks.insert(block.batch_number, block);
        {
            inner.current_timestamp += 1;
            inner.current_batch += 1;
            inner.current_miniblock += 1;
        }
    }
}

impl EthNamespaceT for InMemoryNode {
    fn chain_id(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::U64>> {
        let inner = self.inner.read().unwrap();
        Ok(U64::from(inner.fork_storage.chain_id.0 as u64)).into_boxed_future()
    }

    fn call(
        &self,
        req: zksync_types::transaction_request::CallRequest,
        _block: Option<zksync_types::api::BlockIdVariant>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::Bytes>> {
        let mut tx = L2Tx::from_request(req.into(), MAX_TX_SIZE).unwrap();
        tx.common_data.fee.gas_limit = ETH_CALL_GAS_LIMIT.into();
        let result = self.run_l2_call(tx);

        Ok(result.into()).into_boxed_future()
    }

    fn get_balance(
        &self,
        address: zksync_basic_types::Address,
        _block: Option<zksync_types::api::BlockIdVariant>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        let balance_key = storage_key_for_standard_token_balance(
            AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
            &address,
        );

        let balance = self
            .inner
            .write()
            .unwrap()
            .fork_storage
            .read_value(&balance_key);

        Ok(h256_to_u256(balance)).into_boxed_future()
    }

    fn get_block_by_number(
        &self,
        block_number: zksync_types::api::BlockNumber,
        _full_transactions: bool,
    ) -> jsonrpc_core::BoxFuture<
        jsonrpc_core::Result<
            Option<zksync_types::api::Block<zksync_types::api::TransactionVariant>>,
        >,
    > {
        // Currently we support only the 'most recent' block.
        let reader = self.inner.read().unwrap();
        match block_number {
            zksync_types::api::BlockNumber::Committed
            | zksync_types::api::BlockNumber::Finalized
            | zksync_types::api::BlockNumber::Latest => {}
            zksync_types::api::BlockNumber::Earliest
            | zksync_types::api::BlockNumber::Pending
            | zksync_types::api::BlockNumber::Number(_) => return not_implemented(),
        }

        let txn: Vec<TransactionVariant> = vec![];

        let block = zksync_types::api::Block {
            transactions: txn,
            hash: Default::default(),
            parent_hash: Default::default(),
            uncles_hash: Default::default(),
            author: Default::default(),
            state_root: Default::default(),
            transactions_root: Default::default(),
            receipts_root: Default::default(),
            number: U64::from(reader.current_miniblock),
            l1_batch_number: Some(U64::from(reader.current_batch)),
            gas_used: Default::default(),
            gas_limit: Default::default(),
            base_fee_per_gas: Default::default(),
            extra_data: Default::default(),
            logs_bloom: Default::default(),
            timestamp: Default::default(),
            l1_batch_timestamp: Default::default(),
            difficulty: Default::default(),
            total_difficulty: Default::default(),
            seal_fields: Default::default(),
            uncles: Default::default(),
            size: Default::default(),
            mix_hash: Default::default(),
            nonce: Default::default(),
        };

        Ok(Some(block)).into_boxed_future()
    }

    fn get_code(
        &self,
        address: zksync_basic_types::Address,
        _block: Option<zksync_types::api::BlockIdVariant>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::Bytes>> {
        let code_key = get_code_key(&address);

        let code_hash = self
            .inner
            .write()
            .unwrap()
            .fork_storage
            .read_value(&code_key);

        Ok(Bytes::from(code_hash.as_bytes())).into_boxed_future()
    }

    fn get_transaction_count(
        &self,
        address: zksync_basic_types::Address,
        _block: Option<zksync_types::api::BlockIdVariant>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        let nonce_key = get_nonce_key(&address);

        let result = self
            .inner
            .write()
            .unwrap()
            .fork_storage
            .read_value(&nonce_key);
        Ok(h256_to_u64(result).into()).into_boxed_future()
    }

    fn get_transaction_receipt(
        &self,
        hash: zksync_basic_types::H256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<zksync_types::api::TransactionReceipt>>>
    {
        let reader = self.inner.read().unwrap();
        let tx_result = reader.tx_results.get(&hash);

        let receipt = tx_result.map(|info| {
            let status = if info.result.status == TxExecutionStatus::Success {
                U64::from(1)
            } else {
                U64::from(0)
            };

            TransactionReceipt {
                transaction_hash: hash,
                transaction_index: U64::from(1),
                block_hash: None,
                block_number: Some(U64::from(info.miniblock_number)),
                l1_batch_tx_index: None,
                l1_batch_number: Some(U64::from(info.batch_number as u64)),
                from: Default::default(),
                to: Some(info.tx.execute.contract_address),
                cumulative_gas_used: Default::default(),
                gas_used: Some(info.tx.common_data.fee.gas_limit - info.result.gas_refunded),
                contract_address: contract_address_from_tx_result(&info.result),
                logs: vec![],
                l2_to_l1_logs: vec![],
                status: Some(status),
                root: None,
                logs_bloom: Default::default(),
                transaction_type: None,
                effective_gas_price: Some(500.into()),
            }
        });

        Ok(receipt).into_boxed_future()
    }

    fn send_raw_transaction(
        &self,
        tx_bytes: zksync_basic_types::Bytes,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::H256>> {
        let chain_id = TEST_NODE_NETWORK_ID;
        let (tx_req, hash) = TransactionRequest::from_bytes(&tx_bytes.0, chain_id).unwrap();

        let mut l2_tx: L2Tx = L2Tx::from_request(tx_req, MAX_TX_SIZE).unwrap();
        l2_tx.set_input(tx_bytes.0, hash);
        assert_eq!(hash, l2_tx.hash());

        self.run_l2_tx(l2_tx, TxExecutionMode::VerifyExecute);

        Ok(hash).into_boxed_future()
    }

    // Methods below are not currently implemented.

    fn get_block_number(
        &self,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::U64>> {
        not_implemented()
    }

    fn estimate_gas(
        &self,
        _req: zksync_types::transaction_request::CallRequest,
        _block: Option<zksync_types::api::BlockNumber>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn gas_price(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn new_filter(&self, _filter: Filter) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn new_block_filter(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn uninstall_filter(&self, _idx: U256) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<bool>> {
        not_implemented()
    }

    fn new_pending_transaction_filter(
        &self,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn get_logs(
        &self,
        _filter: Filter,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Vec<zksync_types::api::Log>>> {
        not_implemented()
    }

    fn get_filter_logs(
        &self,
        _filter_index: U256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<FilterChanges>> {
        not_implemented()
    }

    fn get_filter_changes(
        &self,
        _filter_index: U256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<FilterChanges>> {
        not_implemented()
    }

    fn get_block_by_hash(
        &self,
        _hash: zksync_basic_types::H256,
        _full_transactions: bool,
    ) -> jsonrpc_core::BoxFuture<
        jsonrpc_core::Result<
            Option<zksync_types::api::Block<zksync_types::api::TransactionVariant>>,
        >,
    > {
        not_implemented()
    }

    fn get_block_transaction_count_by_number(
        &self,
        _block_number: zksync_types::api::BlockNumber,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<U256>>> {
        not_implemented()
    }

    fn get_block_transaction_count_by_hash(
        &self,
        _block_hash: zksync_basic_types::H256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<U256>>> {
        not_implemented()
    }

    fn get_storage(
        &self,
        _address: zksync_basic_types::Address,
        _idx: U256,
        _block: Option<zksync_types::api::BlockIdVariant>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::H256>> {
        not_implemented()
    }

    fn get_transaction_by_hash(
        &self,
        _hash: zksync_basic_types::H256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<zksync_types::api::Transaction>>> {
        not_implemented()
    }

    fn get_transaction_by_block_hash_and_index(
        &self,
        _block_hash: zksync_basic_types::H256,
        _index: zksync_basic_types::web3::types::Index,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<zksync_types::api::Transaction>>> {
        not_implemented()
    }

    fn get_transaction_by_block_number_and_index(
        &self,
        _block_number: zksync_types::api::BlockNumber,
        _index: zksync_basic_types::web3::types::Index,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<zksync_types::api::Transaction>>> {
        not_implemented()
    }

    fn protocol_version(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<String>> {
        not_implemented()
    }

    fn syncing(
        &self,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::web3::types::SyncState>>
    {
        not_implemented()
    }

    fn accounts(
        &self,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Vec<zksync_basic_types::Address>>> {
        not_implemented()
    }

    fn coinbase(
        &self,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<zksync_basic_types::Address>> {
        not_implemented()
    }

    fn compilers(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Vec<String>>> {
        not_implemented()
    }

    fn hashrate(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<U256>> {
        not_implemented()
    }

    fn get_uncle_count_by_block_hash(
        &self,
        _hash: zksync_basic_types::H256,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<U256>>> {
        not_implemented()
    }

    fn get_uncle_count_by_block_number(
        &self,
        _number: zksync_types::api::BlockNumber,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<Option<U256>>> {
        not_implemented()
    }

    fn mining(&self) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<bool>> {
        not_implemented()
    }

    fn fee_history(
        &self,
        _block_count: U64,
        _newest_block: BlockNumber,
        _reward_percentiles: Vec<f32>,
    ) -> jsonrpc_core::BoxFuture<jsonrpc_core::Result<FeeHistory>> {
        not_implemented()
    }
}

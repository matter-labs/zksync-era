// //!
// //! Tests for the bootloader
// //! The description for each of the tests can be found in the corresponding `.yul` file.
// //!
// #![cfg_attr(test, allow(unused_imports))]

// use crate::errors::{VmRevertReason, VmRevertReasonParsingResult};
// use crate::memory::SimpleMemory;
// use crate::oracles::tracer::{
//     read_pointer, ExecutionEndTracer, PendingRefundTracer, PubdataSpentTracer,
//     TransactionResultTracer, VmHook,
// };
// use crate::storage::{Storage, StoragePtr};
// use crate::test_utils::{
//     get_create_execute, get_create_zksync_address, get_deploy_tx, get_error_tx,
//     mock_loadnext_test_call, VmInstanceInnerState,
// };
// use crate::utils::{
//     create_test_block_params, insert_system_contracts, read_bootloader_test_code,
//     BASE_SYSTEM_CONTRACTS, BLOCK_GAS_LIMIT,
// };
// use crate::vm::{
//     get_vm_hook_params, tx_has_failed, VmBlockResult, VmExecutionStopReason, ZkSyncVmState,
//     MAX_MEM_SIZE_BYTES,
// };
// use crate::vm_with_bootloader::{
//     bytecode_to_factory_dep, get_bootloader_memory, get_bootloader_memory_for_encoded_tx,
//     init_vm_inner, push_raw_transaction_to_bootloader_memory,
//     push_transaction_to_bootloader_memory, BlockContext, DerivedBlockContext, BOOTLOADER_HEAP_PAGE,
//     BOOTLOADER_TX_DESCRIPTION_OFFSET, TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET,
// };
// use crate::vm_with_bootloader::{BlockContextMode, BootloaderJobType, TxExecutionMode};
// use crate::{test_utils, VmInstance};
// use crate::{TxRevertReason, VmExecutionResult};
// use itertools::Itertools;
// use std::cell::RefCell;
// use std::convert::TryFrom;
// use std::ops::{Add, DivAssign};
// use std::rc::Rc;
// use tempfile::TempDir;
// use zk_evm_1_3_1::abstractions::{
//     AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
//     MAX_HEAP_PAGE_SIZE_IN_WORDS, MAX_MEMORY_BYTES,
// };
// use zk_evm_1_3_1::aux_structures::Timestamp;
// use zk_evm_1_3_1::block_properties::BlockProperties;
// use zk_evm_1_3_1::sha3::digest::typenum::U830;
// use zk_evm_1_3_1::witness_trace::VmWitnessTracer;
// use zk_evm_1_3_1::zkevm_opcode_defs::decoding::VmEncodingMode;
// use zk_evm_1_3_1::zkevm_opcode_defs::FatPointer;
// use zksync_types::block::DeployedContract;
// use zksync_types::ethabi::encode;
// use zksync_types::l1::L1Tx;
// use zksync_types::tx::tx_execution_info::{TxExecutionStatus, VmExecutionLogs};
// use zksync_utils::test_utils::LoadnextContractExecutionParams;
// use zksync_utils::{
//     address_to_h256, bytecode::hash_bytecode, bytes_to_be_words, bytes_to_le_words, h256_to_u256,
//     u256_to_h256,
// };
// use zksync_utils::{h256_to_account_address, u256_to_account_address};

// use crate::{transaction_data::TransactionData, OracleTools};
// use std::time;
// use zksync_contracts::{
//     default_erc20_bytecode, get_loadnext_contract, known_codes_contract, load_contract,
//     load_sys_contract, read_bootloader_code, read_bytecode, read_zbin_bytecode,
//     BaseSystemContracts, SystemContractCode, PLAYGROUND_BLOCK_BOOTLOADER_CODE,
// };
// use zksync_crypto::rand::random;
// use zksync_state::secondary_storage::SecondaryStateStorage;
// use zksync_state::storage_view::StorageView;
// use zksync_storage::db::Database;
// use zksync_storage::RocksDB;
// use zksync_types::system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT};
// use zksync_types::utils::{
//     deployed_address_create, storage_key_for_eth_balance, storage_key_for_standard_token_balance,
// };
// use zksync_types::{
//     ethabi::Token, AccountTreeId, Address, Execute, ExecuteTransactionCommon, L1BatchNumber,
//     L2ChainId, PackedEthSignature, StorageKey, StorageLogQueryType, Transaction, H256,
//     KNOWN_CODES_STORAGE_ADDRESS, U256,
// };
// use zksync_types::{fee::Fee, l2::L2Tx, l2_to_l1_log::L2ToL1Log, tx::ExecutionMetrics};
// use zksync_types::{
//     get_code_key, get_is_account_key, get_known_code_key, get_nonce_key, L1TxCommonData, Nonce,
//     PriorityOpId, SerialId, StorageLog, ZkSyncReadStorage, BOOTLOADER_ADDRESS,
//     CONTRACT_DEPLOYER_ADDRESS, H160, L2_ETH_TOKEN_ADDRESS, MAX_GAS_PER_PUBDATA_BYTE,
//     MAX_TXS_IN_BLOCK, SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_GAS_PRICE_POSITION,
//     SYSTEM_CONTEXT_MINIMAL_BASE_FEE, SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
// };

// use once_cell::sync::Lazy;
// use zksync_system_constants::ZKPORTER_IS_AVAILABLE;

// fn run_vm_with_custom_factory_deps<'a>(
//     oracle_tools: &'a mut OracleTools<'a, false>,
//     block_context: BlockContext,
//     block_properties: &'a BlockProperties,
//     encoded_tx: Vec<U256>,
//     predefined_overhead: u32,
//     expected_error: Option<TxRevertReason>,
// ) {
//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
//     base_system_contracts.bootloader = PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone();
//     let mut vm = init_vm_inner(
//         oracle_tools,
//         BlockContextMode::OverrideCurrent(block_context.into()),
//         block_properties,
//         BLOCK_GAS_LIMIT,
//         &base_system_contracts,
//         TxExecutionMode::VerifyExecute,
//     );

//     vm.bootloader_state.add_tx_data(encoded_tx.len());
//     vm.state.memory.populate_page(
//         BOOTLOADER_HEAP_PAGE as usize,
//         get_bootloader_memory_for_encoded_tx(
//             encoded_tx,
//             0,
//             TxExecutionMode::VerifyExecute,
//             0,
//             0,
//             predefined_overhead,
//         ),
//         Timestamp(0),
//     );

//     let result = vm.execute_next_tx().err();

//     assert_eq!(expected_error, result);
// }

// fn get_balance(token_id: AccountTreeId, account: &Address, main_storage: StoragePtr<'_>) -> U256 {
//     let key = storage_key_for_standard_token_balance(token_id, account);
//     h256_to_u256(main_storage.borrow_mut().get_value(&key))
// }

// #[test]
// fn test_dummy_bootloader() {
//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let mut storage_accessor = StorageView::new(&raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut storage_accessor;

//     let mut oracle_tools = OracleTools::new(storage_ptr);
//     let (block_context, block_properties) = create_test_block_params();
//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
//     let bootloader_code = read_bootloader_test_code("dummy");
//     let bootloader_hash = hash_bytecode(&bootloader_code);

//     base_system_contracts.bootloader = SystemContractCode {
//         code: bytes_to_be_words(bootloader_code),
//         hash: bootloader_hash,
//     };

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context.into(), Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &base_system_contracts,
//         TxExecutionMode::VerifyExecute,
//     );

//     let VmBlockResult {
//         full_result: res, ..
//     } = vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing);

//     // Dummy bootloader should not panic
//     assert!(res.revert_reason.is_none());

//     let correct_first_cell = U256::from_str_radix("123123123", 16).unwrap();

//     verify_required_memory(
//         &vm.state,
//         vec![(correct_first_cell, BOOTLOADER_HEAP_PAGE, 0)],
//     );
// }

// #[test]
// fn test_bootloader_out_of_gas() {
//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let mut storage_accessor = StorageView::new(&raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut storage_accessor;

//     let mut oracle_tools = OracleTools::new(storage_ptr);
//     let (block_context, block_properties) = create_test_block_params();

//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();

//     let bootloader_code = read_bootloader_test_code("dummy");
//     let bootloader_hash = hash_bytecode(&bootloader_code);

//     base_system_contracts.bootloader = SystemContractCode {
//         code: bytes_to_be_words(bootloader_code),
//         hash: bootloader_hash,
//     };

//     // init vm with only 100 ergs
//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context.into(), Default::default()),
//         &block_properties,
//         10,
//         &base_system_contracts,
//         TxExecutionMode::VerifyExecute,
//     );

//     let res = vm.execute_block_tip();

//     assert_eq!(res.revert_reason, Some(TxRevertReason::BootloaderOutOfGas));
// }

// fn verify_required_storage(state: &ZkSyncVmState<'_>, required_values: Vec<(H256, StorageKey)>) {
//     for (required_value, key) in required_values {
//         let current_value = state.storage.storage.read_from_storage(&key);

//         assert_eq!(
//             u256_to_h256(current_value),
//             required_value,
//             "Invalid value at key {key:?}"
//         );
//     }
// }

// fn verify_required_memory(state: &ZkSyncVmState<'_>, required_values: Vec<(U256, u32, u32)>) {
//     for (required_value, memory_page, cell) in required_values {
//         let current_value = state
//             .memory
//             .dump_page_content_as_u256_words(memory_page, cell..cell + 1)[0];
//         assert_eq!(current_value, required_value);
//     }
// }

// #[test]
// fn test_default_aa_interaction() {
//     // In this test, we aim to test whether a simple account interaction (without any fee logic)
//     // will work. The account will try to deploy a simple contract from integration tests.

//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     let operator_address = block_context.context.operator_address;
//     let base_fee = block_context.base_fee;
//     // We deploy here counter contract, because its logic is trivial
//     let contract_code = read_test_contract();
//     let contract_code_hash = hash_bytecode(&contract_code);
//     let tx: Transaction = get_deploy_tx(
//         H256::random(),
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();
//     let tx_data: TransactionData = tx.clone().into();

//     let maximal_fee = tx_data.gas_limit * tx_data.max_fee_per_gas;
//     let sender_address = tx_data.from();
//     // set balance

//     let key = storage_key_for_eth_balance(&sender_address);
//     storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);

//     let tx_execution_result = vm
//         .execute_next_tx()
//         .expect("Bootloader failed while processing transaction");

//     assert_eq!(
//         tx_execution_result.status,
//         TxExecutionStatus::Success,
//         "Transaction wasn't successful"
//     );

//     let VmBlockResult {
//         full_result: res, ..
//     } = vm.execute_till_block_end(BootloaderJobType::TransactionExecution);
//     // Should not panic
//     assert!(
//         res.revert_reason.is_none(),
//         "Bootloader was not expected to revert: {:?}",
//         res.revert_reason
//     );

//     // Both deployment and ordinary nonce should be incremented by one.
//     let account_nonce_key = get_nonce_key(&sender_address);
//     let expected_nonce = TX_NONCE_INCREMENT + DEPLOYMENT_NONCE_INCREMENT;

//     // The code hash of the deployed contract should be marked as republished.
//     let known_codes_key = get_known_code_key(&contract_code_hash);

//     // The contract should be deployed successfully.
//     let deployed_address = deployed_address_create(sender_address, U256::zero());
//     let account_code_key = get_code_key(&deployed_address);

//     let expected_slots = vec![
//         (u256_to_h256(expected_nonce), account_nonce_key),
//         (u256_to_h256(U256::from(1u32)), known_codes_key),
//         (contract_code_hash, account_code_key),
//     ];

//     verify_required_storage(&vm.state, expected_slots);

//     assert!(!tx_has_failed(&vm.state, 0));

//     let expected_fee =
//         maximal_fee - U256::from(tx_execution_result.gas_refunded) * U256::from(base_fee);
//     let operator_balance = get_balance(
//         AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
//         &operator_address,
//         vm.state.storage.storage.get_ptr(),
//     );

//     assert!(
//         operator_balance == expected_fee,
//         "Operator did not receive his fee"
//     );
// }

// fn execute_vm_with_predetermined_refund(txs: Vec<Transaction>, refunds: Vec<u32>) -> VmBlockResult {
//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     // set balance
//     for tx in txs.iter() {
//         let sender_address = tx.initiator_account();
//         let key = storage_key_for_eth_balance(&sender_address);
//         storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));
//     }

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );

//     let codes_for_decommiter = txs
//         .iter()
//         .flat_map(|tx| {
//             tx.execute
//                 .factory_deps
//                 .clone()
//                 .unwrap_or_default()
//                 .iter()
//                 .map(|dep| bytecode_to_factory_dep(dep.clone()))
//                 .collect::<Vec<(U256, Vec<U256>)>>()
//         })
//         .collect();

//     vm.state.decommittment_processor.populate(
//         codes_for_decommiter,
//         Timestamp(vm.state.local_state.timestamp),
//     );

//     let memory_with_suggested_refund = get_bootloader_memory(
//         txs.into_iter().map(Into::into).collect(),
//         refunds,
//         TxExecutionMode::VerifyExecute,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//     );

//     vm.state.memory.populate_page(
//         BOOTLOADER_HEAP_PAGE as usize,
//         memory_with_suggested_refund,
//         Timestamp(0),
//     );

//     vm.execute_till_block_end(BootloaderJobType::TransactionExecution)
// }

// #[test]
// fn test_predetermined_refunded_gas() {
//     // In this test, we compare the execution of the bootloader with the predefined
//     // refunded gas and without them

//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     let base_fee = block_context.base_fee;
//     // We deploy here counter contract, because its logic is trivial
//     let contract_code = read_test_contract();
//     let tx: Transaction = get_deploy_tx(
//         H256::random(),
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();

//     let sender_address = tx.initiator_account();

//     // set balance
//     let key = storage_key_for_eth_balance(&sender_address);
//     storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);

//     let tx_execution_result = vm
//         .execute_next_tx()
//         .expect("Bootloader failed while processing transaction");

//     assert_eq!(
//         tx_execution_result.status,
//         TxExecutionStatus::Success,
//         "Transaction wasn't successful"
//     );

//     // If the refund provided by the operator or the final refund are the 0
//     // there is no impact of the operator's refund at all and so this test does not
//     // make much sense.
//     assert!(
//         tx_execution_result.operator_suggested_refund > 0,
//         "The operator's refund is 0"
//     );
//     assert!(
//         tx_execution_result.gas_refunded > 0,
//         "The final refund is 0"
//     );

//     let mut result = vm.execute_till_block_end(BootloaderJobType::TransactionExecution);
//     assert!(
//         result.full_result.revert_reason.is_none(),
//         "Bootloader was not expected to revert: {:?}",
//         result.full_result.revert_reason
//     );

//     let mut result_with_predetermined_refund = execute_vm_with_predetermined_refund(
//         vec![tx],
//         vec![tx_execution_result.operator_suggested_refund],
//     );
//     // We need to sort these lists as those are flattened from HashMaps
//     result.full_result.used_contract_hashes.sort();
//     result_with_predetermined_refund
//         .full_result
//         .used_contract_hashes
//         .sort();

//     assert_eq!(
//         result.full_result.events,
//         result_with_predetermined_refund.full_result.events
//     );
//     assert_eq!(
//         result.full_result.l2_to_l1_logs,
//         result_with_predetermined_refund.full_result.l2_to_l1_logs
//     );
//     assert_eq!(
//         result.full_result.storage_log_queries,
//         result_with_predetermined_refund
//             .full_result
//             .storage_log_queries
//     );
//     assert_eq!(
//         result.full_result.used_contract_hashes,
//         result_with_predetermined_refund
//             .full_result
//             .used_contract_hashes
//     );
// }

// #[derive(Debug, Clone)]
// enum TransactionRollbackTestInfo {
//     Rejected(Transaction, TxRevertReason),
//     Processed(Transaction, bool, TxExecutionStatus),
// }

// impl TransactionRollbackTestInfo {
//     fn new_rejected(transaction: Transaction, revert_reason: TxRevertReason) -> Self {
//         Self::Rejected(transaction, revert_reason)
//     }

//     fn new_processed(
//         transaction: Transaction,
//         should_be_rollbacked: bool,
//         expected_status: TxExecutionStatus,
//     ) -> Self {
//         Self::Processed(transaction, should_be_rollbacked, expected_status)
//     }

//     fn get_transaction(&self) -> &Transaction {
//         match self {
//             TransactionRollbackTestInfo::Rejected(tx, _) => tx,
//             TransactionRollbackTestInfo::Processed(tx, _, _) => tx,
//         }
//     }

//     fn rejection_reason(&self) -> Option<TxRevertReason> {
//         match self {
//             TransactionRollbackTestInfo::Rejected(_, revert_reason) => Some(revert_reason.clone()),
//             TransactionRollbackTestInfo::Processed(_, _, _) => None,
//         }
//     }

//     fn should_rollback(&self) -> bool {
//         match self {
//             TransactionRollbackTestInfo::Rejected(_, _) => true,
//             TransactionRollbackTestInfo::Processed(_, x, _) => *x,
//         }
//     }

//     fn expected_status(&self) -> TxExecutionStatus {
//         match self {
//             TransactionRollbackTestInfo::Rejected(_, _) => {
//                 panic!("There is no execution status for rejected transaction")
//             }
//             TransactionRollbackTestInfo::Processed(_, _, status) => *status,
//         }
//     }
// }

// // Accepts the address of the sender as well as the list of pairs of its transactions
// // and whether these transactions should succeed.
// fn execute_vm_with_possible_rollbacks(
//     sender_address: Address,
//     transactions: Vec<TransactionRollbackTestInfo>,
//     block_context: DerivedBlockContext,
//     block_properties: BlockProperties,
// ) -> VmExecutionResult {
//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     // Setting infinite balance for the sender.
//     let key = storage_key_for_eth_balance(&sender_address);
//     storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );

//     for test_info in transactions {
//         vm.save_current_vm_as_snapshot();
//         let vm_state_before_tx = vm.dump_inner_state();
//         push_transaction_to_bootloader_memory(
//             &mut vm,
//             test_info.get_transaction(),
//             TxExecutionMode::VerifyExecute,
//         );

//         match vm.execute_next_tx() {
//             Err(reason) => {
//                 assert_eq!(test_info.rejection_reason(), Some(reason));
//             }
//             Ok(res) => {
//                 assert_eq!(test_info.rejection_reason(), None);
//                 assert_eq!(
//                     res.status,
//                     test_info.expected_status(),
//                     "Transaction status is not correct"
//                 );
//             }
//         };

//         if test_info.should_rollback() {
//             // Some error has occurred, we should reject the transaction
//             vm.rollback_to_latest_snapshot();

//             // vm_state_before_tx.
//             let state_after_rollback = vm.dump_inner_state();
//             assert_eq!(
//                 vm_state_before_tx, state_after_rollback,
//                 "Did not rollback VM state correctly"
//             );
//         }
//     }

//     let VmBlockResult {
//         full_result: mut result,
//         ..
//     } = vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing);
//     // Used contract hashes are retrieved in unordered manner.
//     // However it must be sorted for the comparisons in tests to work
//     result.used_contract_hashes.sort();

//     result
// }

// // Sets the signature for an L2 transaction and returns the same transaction
// // but this different signature.
// fn change_signature(mut tx: Transaction, signature: Vec<u8>) -> Transaction {
//     tx.common_data = match tx.common_data {
//         ExecuteTransactionCommon::L2(mut data) => {
//             data.signature = signature;
//             ExecuteTransactionCommon::L2(data)
//         }
//         _ => unreachable!(),
//     };

//     tx
// }

// #[test]
// fn test_vm_rollbacks() {
//     let (block_context, block_properties): (DerivedBlockContext, BlockProperties) = {
//         let (block_context, block_properties) = create_test_block_params();
//         (block_context.into(), block_properties)
//     };

//     let base_fee = U256::from(block_context.base_fee);

//     let sender_private_key = H256::random();
//     let contract_code = read_test_contract();

//     let tx_nonce_0: Transaction = get_deploy_tx(
//         sender_private_key,
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: base_fee,
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();
//     let tx_nonce_1: Transaction = get_deploy_tx(
//         sender_private_key,
//         Nonce(1),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: base_fee,
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();
//     let tx_nonce_2: Transaction = get_deploy_tx(
//         sender_private_key,
//         Nonce(2),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: base_fee,
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();

//     let wrong_signature_length_tx = change_signature(tx_nonce_0.clone(), vec![1u8; 32]);
//     let wrong_v_tx = change_signature(tx_nonce_0.clone(), vec![1u8; 65]);
//     let wrong_signature_tx = change_signature(tx_nonce_0.clone(), vec![27u8; 65]);

//     let sender_address = tx_nonce_0.initiator_account();

//     let result_without_rollbacks = execute_vm_with_possible_rollbacks(
//         sender_address,
//         vec![
//             // The nonces are ordered correctly, all the transactions should succeed.
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_0.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_1.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_2.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//         ],
//         block_context,
//         block_properties,
//     );

//     let incorrect_nonce = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Incorrect nonce".to_string(),
//     });
//     let reusing_nonce_twice = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Reusing the same nonce twice".to_string(),
//     });
//     let signature_length_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Signature length is incorrect".to_string(),
//     });
//     let v_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "v is neither 27 nor 28".to_string(),
//     });
//     let signature_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Account validation returned invalid magic value. Most often this means that the signature is incorrect".to_string(),
//     });

//     let result_with_rollbacks = execute_vm_with_possible_rollbacks(
//         sender_address,
//         vec![
//             TransactionRollbackTestInfo::new_rejected(
//                 wrong_signature_length_tx,
//                 signature_length_is_incorrect,
//             ),
//             TransactionRollbackTestInfo::new_rejected(wrong_v_tx, v_is_incorrect),
//             TransactionRollbackTestInfo::new_rejected(wrong_signature_tx, signature_is_incorrect),
//             // The correct nonce is 0, this tx will fail
//             TransactionRollbackTestInfo::new_rejected(tx_nonce_2.clone(), incorrect_nonce.clone()),
//             // This tx will succeed
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_0.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             // The correct nonce is 1, this tx will fail
//             TransactionRollbackTestInfo::new_rejected(
//                 tx_nonce_0.clone(),
//                 reusing_nonce_twice.clone(),
//             ),
//             // The correct nonce is 1, this tx will fail
//             TransactionRollbackTestInfo::new_rejected(tx_nonce_2.clone(), incorrect_nonce),
//             // This tx will succeed
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_1,
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             // The correct nonce is 2, this tx will fail
//             TransactionRollbackTestInfo::new_rejected(tx_nonce_0, reusing_nonce_twice.clone()),
//             // This tx will succeed
//             TransactionRollbackTestInfo::new_processed(
//                 tx_nonce_2.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             // This tx will fail
//             TransactionRollbackTestInfo::new_rejected(tx_nonce_2, reusing_nonce_twice.clone()),
//         ],
//         block_context,
//         block_properties,
//     );

//     assert_eq!(result_without_rollbacks, result_with_rollbacks);

//     let loadnext_contract = get_loadnext_contract();

//     let loadnext_constructor_data = encode(&[Token::Uint(U256::from(100))]);
//     let loadnext_deploy_tx: Transaction = get_deploy_tx(
//         sender_private_key,
//         Nonce(0),
//         &loadnext_contract.bytecode,
//         loadnext_contract.factory_deps,
//         &loadnext_constructor_data,
//         Fee {
//             gas_limit: U256::from(60000000u32),
//             max_fee_per_gas: base_fee,
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();
//     let loadnext_contract_address =
//         get_create_zksync_address(loadnext_deploy_tx.initiator_account(), Nonce(0));
//     let deploy_loadnext_tx_info = TransactionRollbackTestInfo::new_processed(
//         loadnext_deploy_tx,
//         false,
//         TxExecutionStatus::Success,
//     );

//     let get_load_next_tx = |params: LoadnextContractExecutionParams, nonce: Nonce| {
//         // Here we test loadnext with various kinds of operations
//         let tx: Transaction = mock_loadnext_test_call(
//             sender_private_key,
//             nonce,
//             loadnext_contract_address,
//             Fee {
//                 gas_limit: U256::from(80000000u32),
//                 max_fee_per_gas: base_fee,
//                 max_priority_fee_per_gas: U256::zero(),
//                 gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//             },
//             params,
//         )
//         .into();

//         tx
//     };

//     let loadnext_tx_0 = get_load_next_tx(
//         LoadnextContractExecutionParams {
//             reads: 100,
//             writes: 100,
//             events: 100,
//             hashes: 500,
//             recursive_calls: 10,
//             deploys: 60,
//         },
//         Nonce(1),
//     );
//     let loadnext_tx_1 = get_load_next_tx(
//         LoadnextContractExecutionParams {
//             reads: 100,
//             writes: 100,
//             events: 100,
//             hashes: 500,
//             recursive_calls: 10,
//             deploys: 60,
//         },
//         Nonce(2),
//     );

//     let result_without_rollbacks = execute_vm_with_possible_rollbacks(
//         sender_address,
//         vec![
//             deploy_loadnext_tx_info.clone(),
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_0.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_1.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//         ],
//         block_context,
//         block_properties,
//     );

//     let result_with_rollbacks = execute_vm_with_possible_rollbacks(
//         sender_address,
//         vec![
//             deploy_loadnext_tx_info,
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_0.clone(),
//                 true,
//                 TxExecutionStatus::Success,
//             ),
//             // After the previous tx has been rolled back, this one should succeed
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_0.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             // The nonce has been bumped up, this transaction should now fail
//             TransactionRollbackTestInfo::new_rejected(loadnext_tx_0, reusing_nonce_twice.clone()),
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_1.clone(),
//                 true,
//                 TxExecutionStatus::Success,
//             ),
//             // After the previous tx has been rolled back, this one should succeed
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_1.clone(),
//                 true,
//                 TxExecutionStatus::Success,
//             ),
//             // After the previous tx has been rolled back, this one should succeed
//             TransactionRollbackTestInfo::new_processed(
//                 loadnext_tx_1.clone(),
//                 false,
//                 TxExecutionStatus::Success,
//             ),
//             // The nonce has been bumped up, this transaction should now fail
//             TransactionRollbackTestInfo::new_rejected(loadnext_tx_1, reusing_nonce_twice),
//         ],
//         block_context,
//         block_properties,
//     );

//     assert_eq!(result_without_rollbacks, result_with_rollbacks);
// }

// // Inserts the contracts into the test environment, bypassing the
// // deployer system contract. Besides the reference to storage
// // it accepts a `contracts` tuple of information about the contract
// // and whether or not it is an account.
// fn insert_contracts(
//     raw_storage: &mut SecondaryStateStorage,
//     contracts: Vec<(DeployedContract, bool)>,
// ) {
//     let logs: Vec<StorageLog> = contracts
//         .iter()
//         .flat_map(|(contract, is_account)| {
//             let mut new_logs = vec![];

//             let deployer_code_key = get_code_key(contract.account_id.address());
//             new_logs.push(StorageLog::new_write_log(
//                 deployer_code_key,
//                 hash_bytecode(&contract.bytecode),
//             ));

//             if *is_account {
//                 let is_account_key = get_is_account_key(contract.account_id.address());
//                 new_logs.push(StorageLog::new_write_log(
//                     is_account_key,
//                     u256_to_h256(1u32.into()),
//                 ));
//             }

//             new_logs
//         })
//         .collect();
//     raw_storage.process_transaction_logs(&logs);

//     for (contract, _) in contracts {
//         raw_storage.store_contract(*contract.account_id.address(), contract.bytecode.clone());
//         raw_storage.store_factory_dep(hash_bytecode(&contract.bytecode), contract.bytecode);
//     }
//     raw_storage.save(L1BatchNumber(0));
// }

// enum NonceHolderTestMode {
//     SetValueUnderNonce,
//     IncreaseMinNonceBy5,
//     IncreaseMinNonceTooMuch,
//     LeaveNonceUnused,
//     IncreaseMinNonceBy1,
//     SwitchToArbitraryOrdering,
// }

// impl From<NonceHolderTestMode> for u8 {
//     fn from(mode: NonceHolderTestMode) -> u8 {
//         match mode {
//             NonceHolderTestMode::SetValueUnderNonce => 0,
//             NonceHolderTestMode::IncreaseMinNonceBy5 => 1,
//             NonceHolderTestMode::IncreaseMinNonceTooMuch => 2,
//             NonceHolderTestMode::LeaveNonceUnused => 3,
//             NonceHolderTestMode::IncreaseMinNonceBy1 => 4,
//             NonceHolderTestMode::SwitchToArbitraryOrdering => 5,
//         }
//     }
// }

// fn get_nonce_holder_test_tx(
//     nonce: U256,
//     account_address: Address,
//     test_mode: NonceHolderTestMode,
//     block_context: &DerivedBlockContext,
// ) -> TransactionData {
//     TransactionData {
//         tx_type: 113,
//         from: account_address,
//         to: account_address,
//         gas_limit: U256::from(10000000u32),
//         pubdata_price_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         max_fee_per_gas: U256::from(block_context.base_fee),
//         max_priority_fee_per_gas: U256::zero(),
//         nonce,
//         // The reserved fields that are unique for different types of transactions.
//         // E.g. nonce is currently used in all transaction, but it should not be mandatory
//         // in the long run.
//         reserved: [U256::zero(); 4],
//         data: vec![12],
//         signature: vec![test_mode.into()],

//         ..Default::default()
//     }
// }

// fn run_vm_with_raw_tx<'a>(
//     oracle_tools: &'a mut OracleTools<'a, false>,
//     block_context: DerivedBlockContext,
//     block_properties: &'a BlockProperties,
//     tx: TransactionData,
// ) -> (VmExecutionResult, bool) {
//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
//     base_system_contracts.bootloader = PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone();
//     let mut vm = init_vm_inner(
//         oracle_tools,
//         BlockContextMode::OverrideCurrent(block_context),
//         block_properties,
//         BLOCK_GAS_LIMIT,
//         &base_system_contracts,
//         TxExecutionMode::VerifyExecute,
//     );

//     let overhead = tx.overhead_gas();
//     push_raw_transaction_to_bootloader_memory(
//         &mut vm,
//         tx,
//         TxExecutionMode::VerifyExecute,
//         overhead,
//     );
//     let VmBlockResult {
//         full_result: result,
//         ..
//     } = vm.execute_till_block_end(BootloaderJobType::TransactionExecution);

//     (result, tx_has_failed(&vm.state, 0))
// }

// #[test]
// fn test_nonce_holder() {
//     let (block_context, block_properties): (DerivedBlockContext, BlockProperties) = {
//         let (block_context, block_properties) = create_test_block_params();
//         (block_context.into(), block_properties)
//     };

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);

//     let account_address = H160::random();
//     let account = DeployedContract {
//         account_id: AccountTreeId::new(account_address),
//         bytecode: read_nonce_holder_tester(),
//     };

//     insert_contracts(&mut raw_storage, vec![(account, true)]);

//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     // We deploy here counter contract, because its logic is trivial

//     let key = storage_key_for_eth_balance(&account_address);
//     storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut run_nonce_test = |nonce: U256,
//                               test_mode: NonceHolderTestMode,
//                               error_message: Option<String>,
//                               comment: &'static str| {
//         let tx = get_nonce_holder_test_tx(nonce, account_address, test_mode, &block_context);

//         let mut oracle_tools = OracleTools::new(storage_ptr);
//         let (result, tx_has_failed) =
//             run_vm_with_raw_tx(&mut oracle_tools, block_context, &block_properties, tx);
//         if let Some(msg) = error_message {
//             let expected_error = TxRevertReason::ValidationFailed(VmRevertReason::General { msg });
//             assert_eq!(
//                 result
//                     .revert_reason
//                     .expect("No revert reason")
//                     .revert_reason,
//                 expected_error,
//                 "{}",
//                 comment
//             );
//         } else {
//             assert!(!tx_has_failed, "{}", comment);
//         }
//     };

//     // Test 1: trying to set value under non sequential nonce value.
//     run_nonce_test(
//         1u32.into(),
//         NonceHolderTestMode::SetValueUnderNonce,
//         Some("Previous nonce has not been used".to_string()),
//         "Allowed to set value under non sequential value",
//     );

//     // Test 2: increase min nonce by 1 with sequential nonce ordering:
//     run_nonce_test(
//         0u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceBy1,
//         None,
//         "Failed to increment nonce by 1 for sequential account",
//     );

//     // Test 3: correctly set value under nonce with sequential nonce ordering:
//     run_nonce_test(
//         1u32.into(),
//         NonceHolderTestMode::SetValueUnderNonce,
//         None,
//         "Failed to set value under nonce sequential value",
//     );

//     // Test 5: migrate to the arbitrary nonce ordering:
//     run_nonce_test(
//         2u32.into(),
//         NonceHolderTestMode::SwitchToArbitraryOrdering,
//         None,
//         "Failed to switch to arbitrary ordering",
//     );

//     // Test 6: increase min nonce by 5
//     run_nonce_test(
//         6u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceBy5,
//         None,
//         "Failed to increase min nonce by 5",
//     );

//     // Test 7: since the nonces in range [6,10] are no longer allowed, the
//     // tx with nonce 10 should not be allowed
//     run_nonce_test(
//         10u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceBy5,
//         Some("Reusing the same nonce twice".to_string()),
//         "Allowed to reuse nonce below the minimal one",
//     );

//     // Test 8: we should be able to use nonce 13
//     run_nonce_test(
//         13u32.into(),
//         NonceHolderTestMode::SetValueUnderNonce,
//         None,
//         "Did not allow to use unused nonce 10",
//     );

//     // Test 9: we should not be able to reuse nonce 13
//     run_nonce_test(
//         13u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceBy5,
//         Some("Reusing the same nonce twice".to_string()),
//         "Allowed to reuse the same nonce twice",
//     );

//     // Test 10: we should be able to simply use nonce 14, while bumping the minimal nonce by 5
//     run_nonce_test(
//         14u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceBy5,
//         None,
//         "Did not allow to use a bumped nonce",
//     );

//     // Test 6: Do not allow bumping nonce by too much
//     run_nonce_test(
//         16u32.into(),
//         NonceHolderTestMode::IncreaseMinNonceTooMuch,
//         Some("The value for incrementing the nonce is too high".to_string()),
//         "Allowed for incrementing min nonce too much",
//     );

//     // Test 7: Do not allow not setting a nonce as used
//     run_nonce_test(
//         16u32.into(),
//         NonceHolderTestMode::LeaveNonceUnused,
//         Some("The nonce was not set as used".to_string()),
//         "Allowed to leave nonce as unused",
//     );
// }

// #[test]
// fn test_l1_tx_execution() {
//     // In this test, we try to execute a contract deployment from L1
//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let mut storage_accessor = StorageView::new(&raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut storage_accessor;

//     let mut oracle_tools = OracleTools::new(storage_ptr);
//     let (block_context, block_properties) = create_test_block_params();

//     // Here instead of marking code hash via the bootloader means, we will
//     // using L1->L2 communication, the same it would likely be done during the priority mode.
//     let contract_code = read_test_contract();
//     let contract_code_hash = hash_bytecode(&contract_code);
//     let l1_deploy_tx = get_l1_deploy_tx(&contract_code, &[]);
//     let l1_deploy_tx_data: TransactionData = l1_deploy_tx.clone().into();

//     let required_l2_to_l1_logs = vec![
//         L2ToL1Log {
//             shard_id: 0,
//             is_service: false,
//             tx_number_in_block: 0,
//             sender: SYSTEM_CONTEXT_ADDRESS,
//             key: u256_to_h256(U256::from(block_context.block_timestamp)),
//             value: Default::default(),
//         },
//         L2ToL1Log {
//             shard_id: 0,
//             is_service: true,
//             tx_number_in_block: 0,
//             sender: BOOTLOADER_ADDRESS,
//             key: l1_deploy_tx_data.canonical_l1_tx_hash(),
//             value: u256_to_h256(U256::from(1u32)),
//         },
//     ];

//     let sender_address = l1_deploy_tx_data.from();

//     oracle_tools.decommittment_processor.populate(
//         vec![(
//             h256_to_u256(contract_code_hash),
//             bytes_to_be_words(contract_code),
//         )],
//         Timestamp(0),
//     );

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context.into(), Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );
//     push_transaction_to_bootloader_memory(&mut vm, &l1_deploy_tx, TxExecutionMode::VerifyExecute);

//     let res = vm.execute_next_tx().unwrap();

//     // The code hash of the deployed contract should be marked as republished.
//     let known_codes_key = get_known_code_key(&contract_code_hash);

//     // The contract should be deployed successfully.
//     let deployed_address = deployed_address_create(sender_address, U256::zero());
//     let account_code_key = get_code_key(&deployed_address);

//     let expected_slots = vec![
//         (u256_to_h256(U256::from(1u32)), known_codes_key),
//         (contract_code_hash, account_code_key),
//     ];
//     assert!(!tx_has_failed(&vm.state, 0));

//     verify_required_storage(&vm.state, expected_slots);

//     assert_eq!(res.result.logs.l2_to_l1_logs, required_l2_to_l1_logs);

//     let tx = get_l1_execute_test_contract_tx(deployed_address, true);
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);
//     let res = ExecutionMetrics::new(&vm.execute_next_tx().unwrap().result.logs, 0, 0, 0, 0);
//     assert_eq!(res.initial_storage_writes, 0);

//     let tx = get_l1_execute_test_contract_tx(deployed_address, false);
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);
//     let res = ExecutionMetrics::new(&vm.execute_next_tx().unwrap().result.logs, 0, 0, 0, 0);
//     assert_eq!(res.initial_storage_writes, 2);

//     let repeated_writes = res.repeated_storage_writes;

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);
//     let res = ExecutionMetrics::new(&vm.execute_next_tx().unwrap().result.logs, 0, 0, 0, 0);
//     assert_eq!(res.initial_storage_writes, 1);
//     // We do the same storage write, so it will be deduplicated
//     assert_eq!(res.repeated_storage_writes, repeated_writes);

//     let mut tx = get_l1_execute_test_contract_tx(deployed_address, false);
//     tx.execute.value = U256::from(1);
//     match &mut tx.common_data {
//         ExecuteTransactionCommon::L1(l1_data) => {
//             l1_data.to_mint = U256::from(4);
//         }
//         _ => unreachable!(),
//     }
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);
//     let execution_result = vm.execute_next_tx().unwrap();
//     // The method is not payable, so the transaction with non-zero value should fail
//     assert_eq!(
//         execution_result.status,
//         TxExecutionStatus::Failure,
//         "The transaction should fail"
//     );

//     let res = ExecutionMetrics::new(&execution_result.result.logs, 0, 0, 0, 0);

//     // There are 2 initial writes here:
//     // - totalSupply of ETH token
//     // - balance of the refund recipient
//     assert_eq!(res.initial_storage_writes, 2);
// }

// #[test]
// fn test_invalid_bytecode() {
//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let (block_context, block_properties) = create_test_block_params();

//     let test_vm_with_custom_bytecode_hash =
//         |bytecode_hash: H256, expected_revert_reason: Option<TxRevertReason>| {
//             let mut storage_accessor = StorageView::new(&raw_storage);
//             let storage_ptr: &mut dyn Storage = &mut storage_accessor;
//             let mut oracle_tools = OracleTools::new(storage_ptr);

//             let (encoded_tx, predefined_overhead) =
//                 get_l1_tx_with_custom_bytecode_hash(h256_to_u256(bytecode_hash));

//             run_vm_with_custom_factory_deps(
//                 &mut oracle_tools,
//                 block_context,
//                 &block_properties,
//                 encoded_tx,
//                 predefined_overhead,
//                 expected_revert_reason,
//             );
//         };

//     let failed_to_mark_factory_deps = |msg: &str| {
//         TxRevertReason::FailedToMarkFactoryDependencies(VmRevertReason::General {
//             msg: msg.to_string(),
//         })
//     };

//     // Here we provide the correctly-formatted bytecode hash of
//     // odd length, so it should work.
//     test_vm_with_custom_bytecode_hash(
//         H256([
//             1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0,
//         ]),
//         None,
//     );

//     // Here we provide correctly formatted bytecode of even length, so
//     // it should fail.
//     test_vm_with_custom_bytecode_hash(
//         H256([
//             1, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0,
//         ]),
//         Some(failed_to_mark_factory_deps(
//             "Code length in words must be odd",
//         )),
//     );

//     // Here we provide incorrectly formatted bytecode of odd length, so
//     // it should fail.
//     test_vm_with_custom_bytecode_hash(
//         H256([
//             1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0,
//         ]),
//         Some(failed_to_mark_factory_deps(
//             "Incorrectly formatted bytecodeHash",
//         )),
//     );

//     // Here we provide incorrectly formatted bytecode of odd length, so
//     // it should fail.
//     test_vm_with_custom_bytecode_hash(
//         H256([
//             2, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0,
//         ]),
//         Some(failed_to_mark_factory_deps(
//             "Incorrectly formatted bytecodeHash",
//         )),
//     );
// }

// #[test]
// fn test_tracing_of_execution_errors() {
//     // In this test, we are checking that the execution errors are transmitted correctly from the bootloader.
//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);

//     let contract_address = Address::random();
//     let error_contract = DeployedContract {
//         account_id: AccountTreeId::new(contract_address),
//         bytecode: read_error_contract(),
//     };

//     let tx = get_error_tx(
//         H256::random(),
//         Nonce(0),
//         contract_address,
//         Fee {
//             gas_limit: U256::from(1000000u32),
//             max_fee_per_gas: U256::from(10000000000u64),
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(50000u32),
//         },
//     );

//     insert_contracts(&mut raw_storage, vec![(error_contract, false)]);

//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     let key = storage_key_for_eth_balance(&tx.common_data.initiator_address);
//     storage_ptr.set_value(&key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );
//     push_transaction_to_bootloader_memory(&mut vm, &tx.into(), TxExecutionMode::VerifyExecute);

//     let mut tracer = TransactionResultTracer::default();
//     assert_eq!(
//         vm.execute_with_custom_tracer(&mut tracer),
//         VmExecutionStopReason::VmFinished,
//         "Tracer should never request stop"
//     );

//     match tracer.revert_reason {
//         Some(revert_reason) => {
//             let revert_reason = VmRevertReason::try_from(&revert_reason as &[u8]).unwrap();
//             assert_eq!(
//                 revert_reason,
//                 VmRevertReason::General {
//                     msg: "short".to_string()
//                 }
//             )
//         }
//         _ => panic!(
//             "Tracer captured incorrect result {:#?}",
//             tracer.revert_reason
//         ),
//     }
// }

// /// Checks that `TX_GAS_LIMIT_OFFSET` constant is correct.
// #[test]
// fn test_tx_gas_limit_offset() {
//     let gas_limit = U256::from(999999);

//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let raw_storage = SecondaryStateStorage::new(db);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     let contract_code = read_test_contract();
//     let tx: Transaction = get_deploy_tx(
//         H256::random(),
//         Nonce(0),
//         &contract_code,
//         Default::default(),
//         Default::default(),
//         Fee {
//             gas_limit,
//             ..Default::default()
//         },
//     )
//     .into();

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);

//     let gas_limit_from_memory = vm
//         .state
//         .memory
//         .read_slot(
//             BOOTLOADER_HEAP_PAGE as usize,
//             TX_DESCRIPTION_OFFSET + TX_GAS_LIMIT_OFFSET,
//         )
//         .value;
//     assert_eq!(gas_limit_from_memory, gas_limit);
// }

// #[test]
// fn test_is_write_initial_behaviour() {
//     // In this test, we check result of `is_write_initial` at different stages.

//     let (block_context, block_properties) = create_test_block_params();
//     let block_context: DerivedBlockContext = block_context.into();

//     let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
//     let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
//     let mut raw_storage = SecondaryStateStorage::new(db);
//     insert_system_contracts(&mut raw_storage);
//     let storage_ptr: &mut dyn Storage = &mut StorageView::new(&raw_storage);

//     let base_fee = block_context.base_fee;
//     let account_pk = H256::random();
//     let contract_code = read_test_contract();
//     let tx: Transaction = get_deploy_tx(
//         account_pk,
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(10000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();

//     let sender_address = tx.initiator_account();
//     let nonce_key = get_nonce_key(&sender_address);

//     // Check that the next write to the nonce key will be initial.
//     assert!(storage_ptr.is_write_initial(&nonce_key));

//     // Set balance to be able to pay fee for txs.
//     let balance_key = storage_key_for_eth_balance(&sender_address);
//     storage_ptr.set_value(&balance_key, u256_to_h256(U256([0, 0, 1, 0])));

//     let mut oracle_tools = OracleTools::new(storage_ptr);

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(block_context, Default::default()),
//         &block_properties,
//         BLOCK_GAS_LIMIT,
//         &BASE_SYSTEM_CONTRACTS,
//         TxExecutionMode::VerifyExecute,
//     );

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute);

//     vm.execute_next_tx()
//         .expect("Bootloader failed while processing the first transaction");
//     // Check that `is_write_initial` still returns true for the nonce key.
//     assert!(storage_ptr.is_write_initial(&nonce_key));
// }

// pub fn get_l1_tx_with_custom_bytecode_hash(bytecode_hash: U256) -> (Vec<U256>, u32) {
//     let tx: TransactionData = get_l1_execute_test_contract_tx(Default::default(), false).into();
//     let predefined_overhead = tx.overhead_gas_with_custom_factory_deps(vec![bytecode_hash]);
//     let tx_bytes = tx.abi_encode_with_custom_factory_deps(vec![bytecode_hash]);

//     (bytes_to_be_words(tx_bytes), predefined_overhead)
// }

// const L1_TEST_GAS_PER_PUBDATA_BYTE: u32 = 800;

// pub fn get_l1_execute_test_contract_tx(deployed_address: Address, with_panic: bool) -> Transaction {
//     let execute = execute_test_contract(deployed_address, with_panic);
//     Transaction {
//         common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
//             sender: H160::random(),
//             gas_limit: U256::from(1000000u32),
//             gas_per_pubdata_limit: L1_TEST_GAS_PER_PUBDATA_BYTE.into(),
//             ..Default::default()
//         }),
//         execute,
//         received_timestamp_ms: 0,
//     }
// }

// pub fn get_l1_deploy_tx(code: &[u8], calldata: &[u8]) -> Transaction {
//     let execute = get_create_execute(code, calldata);

//     Transaction {
//         common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
//             sender: H160::random(),
//             gas_limit: U256::from(2000000u32),
//             gas_per_pubdata_limit: L1_TEST_GAS_PER_PUBDATA_BYTE.into(),
//             ..Default::default()
//         }),
//         execute,
//         received_timestamp_ms: 0,
//     }
// }

// fn read_test_contract() -> Vec<u8> {
//     read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json")
// }

// fn read_nonce_holder_tester() -> Vec<u8> {
//     read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/custom-account/nonce-holder-test.sol/NonceHolderTest.json")
// }

// fn read_error_contract() -> Vec<u8> {
//     read_bytecode(
//         "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
//     )
// }

// fn execute_test_contract(address: Address, with_panic: bool) -> Execute {
//     let test_contract = load_contract(
//         "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
//     );

//     let function = test_contract.function("incrementWithRevert").unwrap();

//     let calldata = function
//         .encode_input(&[Token::Uint(U256::from(1u8)), Token::Bool(with_panic)])
//         .expect("failed to encode parameters");
//     Execute {
//         contract_address: address,
//         calldata,
//         value: U256::zero(),
//         factory_deps: None,
//     }
// }

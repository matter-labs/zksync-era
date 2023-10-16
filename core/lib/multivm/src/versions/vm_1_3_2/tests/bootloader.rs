// //!
// //! Tests for the bootloader
// //! The description for each of the tests can be found in the corresponding `.yul` file.
// //!
// use itertools::Itertools;
// use std::{
//     collections::{HashMap, HashSet},
//     convert::{TryFrom, TryInto},
// };
// use zksync_eth_signer::{raw_ethereum_tx::TransactionParameters, EthereumSigner, PrivateKeySigner};

// use crate::{
//     errors::VmRevertReason,
//     history_recorder::HistoryMode,
//     oracles::tracer::{StorageInvocationTracer, TransactionResultTracer},
//     test_utils::{
//         get_create_zksync_address, get_deploy_tx, get_error_tx, mock_loadnext_test_call,
//         verify_required_storage,
//     },
//     tests::utils::{
//         get_l1_deploy_tx, get_l1_execute_test_contract_tx_with_sender, read_error_contract,
//         read_long_return_data_contract, read_test_contract,
//     },
//     transaction_data::TransactionData,
//     utils::{
//         create_test_block_params, read_bootloader_test_code, BASE_SYSTEM_CONTRACTS, BLOCK_GAS_LIMIT,
//     },
//     vm::{tx_has_failed, VmExecutionStopReason, ZkSyncVmState},
//     vm_with_bootloader::{
//         bytecode_to_factory_dep, get_bootloader_memory, get_bootloader_memory_for_encoded_tx,
//         push_raw_transaction_to_bootloader_memory, BlockContext, BlockContextMode,
//         BootloaderJobType, TxExecutionMode,
//     },
//     vm_with_bootloader::{
//         init_vm_inner, push_transaction_to_bootloader_memory, DerivedBlockContext,
//         BOOTLOADER_HEAP_PAGE, TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET,
//     },
//     HistoryEnabled, OracleTools, TxRevertReason, VmBlockResult, VmExecutionResult, VmInstance,
// };

// use zk_evm_1_3_3::{
//     aux_structures::Timestamp, block_properties::BlockProperties, zkevm_opcode_defs::FarCallOpcode,
// };
// use zksync_state::{InMemoryStorage, ReadStorage, StoragePtr, StorageView, WriteStorage};
// use zksync_types::{
//     block::DeployedContract,
//     ethabi::encode,
//     ethabi::Token,
//     fee::Fee,
//     get_code_key, get_is_account_key, get_known_code_key, get_nonce_key,
//     l2::L2Tx,
//     l2_to_l1_log::L2ToL1Log,
//     storage_writes_deduplicator::StorageWritesDeduplicator,
//     system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
//     transaction_request::TransactionRequest,
//     tx::tx_execution_info::TxExecutionStatus,
//     utils::{
//         deployed_address_create, storage_key_for_eth_balance,
//         storage_key_for_standard_token_balance,
//     },
//     vm_trace::{Call, CallType},
//     AccountTreeId, Address, Eip712Domain, Execute, ExecuteTransactionCommon, L1TxCommonData,
//     L2ChainId, Nonce, PackedEthSignature, Transaction, BOOTLOADER_ADDRESS, H160, H256,
//     L1_MESSENGER_ADDRESS, L2_ETH_TOKEN_ADDRESS, MAX_GAS_PER_PUBDATA_BYTE,
//     REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, SYSTEM_CONTEXT_ADDRESS, U256,
// };
// use zksync_utils::{
//     bytecode::CompressedBytecodeInfo,
//     test_utils::LoadnextContractExecutionParams,
//     {bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256},
// };

// use zksync_contracts::{
//     get_loadnext_contract, load_contract, SystemContractCode, PLAYGROUND_BLOCK_BOOTLOADER_CODE,
// };

// use super::utils::{read_many_owners_custom_account_contract, read_nonce_holder_tester};
// /// Helper struct for tests, that takes care of setting the database and provides some functions to get and set balances.
// /// Example use:
// ///```ignore
// ///      let test_env = VmTestEnv::default();
// ///      test_env.set_rich_account(address);
// ///      // To create VM and run a single transaction:
// ///      test_env.run_vm_or_die(transaction_data);
// ///      // To create VM:
// ///      let mut helper = VmTestHelper::new(&test_env);
// ///      let mut vm = helper.vm();
// /// ```
// #[derive(Debug)]
// pub struct VmTestEnv {
//     pub block_context: DerivedBlockContext,
//     pub block_properties: BlockProperties,
//     pub storage_ptr: Box<StorageView<InMemoryStorage>>,
// }

// impl VmTestEnv {
//     /// Creates a new test helper with a bunch of already deployed contracts.
//     pub fn new_with_contracts(contracts: &[(H160, Vec<u8>)]) -> Self {
//         let (block_context, block_properties): (DerivedBlockContext, BlockProperties) = {
//             let (block_context, block_properties) = create_test_block_params();
//             (block_context.into(), block_properties)
//         };

//         let mut raw_storage = InMemoryStorage::with_system_contracts(hash_bytecode);
//         for (address, bytecode) in contracts {
//             let account = DeployedContract {
//                 account_id: AccountTreeId::new(*address),
//                 bytecode: bytecode.clone(),
//             };

//             insert_contracts(&mut raw_storage, vec![(account, true)]);
//         }

//         let storage_ptr = Box::new(StorageView::new(raw_storage));

//         VmTestEnv {
//             block_context,
//             block_properties,
//             storage_ptr,
//         }
//     }

//     /// Gets the current ETH balance for a given account.
//     pub fn get_eth_balance(&mut self, address: &H160) -> U256 {
//         get_eth_balance(address, self.storage_ptr.as_mut())
//     }

//     /// Sets a large balance for a given account.
//     pub fn set_rich_account(&mut self, address: &H160) {
//         let key = storage_key_for_eth_balance(address);

//         self.storage_ptr
//             .set_value(key, u256_to_h256(U256::from(10u64.pow(19))));
//     }

//     /// Runs a given transaction in a VM.
//     // Note: that storage changes will be preserved, but not changed to events etc.
//     // Strongly suggest to use this function only if this is the only transaction executed within the test.
//     pub fn run_vm(&mut self, transaction_data: TransactionData) -> (VmExecutionResult, bool) {
//         let mut oracle_tools = OracleTools::new(self.storage_ptr.as_mut(), HistoryEnabled);
//         let (result, tx_has_failed) = run_vm_with_raw_tx(
//             &mut oracle_tools,
//             self.block_context,
//             &self.block_properties,
//             transaction_data,
//         );
//         (result, tx_has_failed)
//     }

//     /// Runs a given transaction in a VM and asserts if it fails.
//     pub fn run_vm_or_die(&mut self, transaction_data: TransactionData) {
//         let (result, tx_has_failed) = self.run_vm(transaction_data);
//         assert!(
//             !tx_has_failed,
//             "Transaction failed with: {:?}",
//             result.revert_reason
//         );
//     }
// }

// impl Default for VmTestEnv {
//     fn default() -> Self {
//         VmTestEnv::new_with_contracts(&[])
//     }
// }

// /// Helper struct to create a default VM for a given environment.
// #[derive(Debug)]
// pub struct VmTestHelper<'a> {
//     pub oracle_tools: OracleTools<'a, false, HistoryEnabled>,
//     pub block_context: DerivedBlockContext,
//     pub block_properties: BlockProperties,
//     vm_created: bool,
// }

// impl<'a> VmTestHelper<'a> {
//     pub fn new(test_env: &'a mut VmTestEnv) -> Self {
//         let block_context = test_env.block_context;
//         let block_properties = test_env.block_properties;

//         let oracle_tools = OracleTools::new(test_env.storage_ptr.as_mut(), HistoryEnabled);
//         VmTestHelper {
//             oracle_tools,
//             block_context,
//             block_properties,
//             vm_created: false,
//         }
//     }

//     /// Creates the VM that can be used in tests.
//     pub fn vm(&'a mut self) -> Box<VmInstance<'a, HistoryEnabled>> {
//         assert!(!self.vm_created, "Vm can be created only once");
//         let vm = init_vm_inner(
//             &mut self.oracle_tools,
//             BlockContextMode::NewBlock(self.block_context, Default::default()),
//             &self.block_properties,
//             BLOCK_GAS_LIMIT,
//             &BASE_SYSTEM_CONTRACTS,
//             TxExecutionMode::VerifyExecute,
//         );
//         self.vm_created = true;
//         vm
//     }
// }

// fn run_vm_with_custom_factory_deps<'a, H: HistoryMode>(
//     oracle_tools: &'a mut OracleTools<'a, false, H>,
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
//             u32::MAX,
//             0,
//             vec![],
//         ),
//         Timestamp(0),
//     );

//     let result = vm.execute_next_tx(u32::MAX, false).err();

//     assert_eq!(expected_error, result);
// }

// fn get_balance(token_id: AccountTreeId, account: &Address, main_storage: StoragePtr) -> U256 {
//     let key = storage_key_for_standard_token_balance(token_id, account);
//     h256_to_u256(main_storage.borrow_mut().read_value(&key))
// }

// fn get_eth_balance(account: &Address, main_storage: &mut StorageView<InMemoryStorage>) -> U256 {
//     let key =
//         storage_key_for_standard_token_balance(AccountTreeId::new(L2_ETH_TOKEN_ADDRESS), account);
//     h256_to_u256(main_storage.read_value(&key))
// }

// #[test]
// fn test_dummy_bootloader() {
//     let mut vm_test_env = VmTestEnv::default();
//     let mut oracle_tools = OracleTools::new(vm_test_env.storage_ptr.as_mut(), HistoryEnabled);
//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
//     let bootloader_code = read_bootloader_test_code("dummy");
//     let bootloader_hash = hash_bytecode(&bootloader_code);

//     base_system_contracts.bootloader = SystemContractCode {
//         code: bytes_to_be_words(bootloader_code),
//         hash: bootloader_hash,
//     };

//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(vm_test_env.block_context, Default::default()),
//         &vm_test_env.block_properties,
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
//     let mut vm_test_env = VmTestEnv::default();
//     let mut oracle_tools = OracleTools::new(vm_test_env.storage_ptr.as_mut(), HistoryEnabled);

//     let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();

//     let bootloader_code = read_bootloader_test_code("dummy");
//     let bootloader_hash = hash_bytecode(&bootloader_code);

//     base_system_contracts.bootloader = SystemContractCode {
//         code: bytes_to_be_words(bootloader_code),
//         hash: bootloader_hash,
//     };

//     // init vm with only 10 ergs
//     let mut vm = init_vm_inner(
//         &mut oracle_tools,
//         BlockContextMode::NewBlock(vm_test_env.block_context, Default::default()),
//         &vm_test_env.block_properties,
//         10,
//         &base_system_contracts,
//         TxExecutionMode::VerifyExecute,
//     );

//     let res = vm.execute_block_tip();

//     assert_eq!(res.revert_reason, Some(TxRevertReason::BootloaderOutOfGas));
// }

// fn verify_required_memory<H: HistoryMode>(
//     state: &ZkSyncVmState<'_, H>,
//     required_values: Vec<(U256, u32, u32)>,
// ) {
//     for (required_value, memory_page, cell) in required_values {
//         let current_value = state
//             .memory
//             .read_slot(memory_page as usize, cell as usize)
//             .value;
//         assert_eq!(current_value, required_value);
//     }
// }

// #[test]
// fn test_default_aa_interaction() {
//     // In this test, we aim to test whether a simple account interaction (without any fee logic)
//     // will work. The account will try to deploy a simple contract from integration tests.

//     let mut vm_test_env = VmTestEnv::default();

//     let operator_address = vm_test_env.block_context.context.operator_address;
//     let base_fee = vm_test_env.block_context.base_fee;
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
//             gas_limit: U256::from(20000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();
//     let tx_data: TransactionData = tx.clone().into();

//     let maximal_fee = tx_data.gas_limit * tx_data.max_fee_per_gas;
//     let sender_address = tx_data.from();

//     vm_test_env.set_rich_account(&sender_address);

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     let tx_execution_result = vm
//         .execute_next_tx(u32::MAX, false)
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

//     assert_eq!(
//         operator_balance, expected_fee,
//         "Operator did not receive his fee"
//     );
// }

// fn execute_vm_with_predetermined_refund(
//     txs: Vec<Transaction>,
//     refunds: Vec<u32>,
//     compressed_bytecodes: Vec<Vec<CompressedBytecodeInfo>>,
// ) -> VmBlockResult {
//     let mut vm_test_env = VmTestEnv::default();
//     let block_context = vm_test_env.block_context;

//     for tx in txs.iter() {
//         let sender_address = tx.initiator_account();
//         vm_test_env.set_rich_account(&sender_address);
//     }

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

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
//         compressed_bytecodes,
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

//     let mut vm_test_env = VmTestEnv::default();
//     let base_fee = vm_test_env.block_context.base_fee;

//     // We deploy here counter contract, because its logic is trivial
//     let contract_code = read_test_contract();
//     let published_bytecode = CompressedBytecodeInfo::from_original(contract_code.clone()).unwrap();
//     let tx: Transaction = get_deploy_tx(
//         H256::random(),
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(20000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();

//     let sender_address = tx.initiator_account();

//     // set balance
//     vm_test_env.set_rich_account(&sender_address);

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     let tx_execution_result = vm
//         .execute_next_tx(u32::MAX, false)
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
//         vec![vec![published_bytecode]],
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
//     let mut vm_test_env = VmTestEnv {
//         block_context,
//         block_properties,
//         ..Default::default()
//     };

//     // Setting infinite balance for the sender.
//     vm_test_env.set_rich_account(&sender_address);

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     for test_info in transactions {
//         vm.save_current_vm_as_snapshot();
//         let vm_state_before_tx = vm.dump_inner_state();
//         push_transaction_to_bootloader_memory(
//             &mut vm,
//             test_info.get_transaction(),
//             TxExecutionMode::VerifyExecute,
//             None,
//         );

//         match vm.execute_next_tx(u32::MAX, false) {
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
//             gas_limit: U256::from(12000000u32),
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
//             gas_limit: U256::from(12000000u32),
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
//             gas_limit: U256::from(12000000u32),
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
//         data: vec![
//             8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 73, 110, 99, 111, 114, 114, 101, 99, 116, 32, 110,
//             111, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//         ],
//     });
//     let reusing_nonce_twice = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Reusing the same nonce twice".to_string(),
//         data: vec![
//             8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 82, 101, 117, 115, 105, 110, 103, 32, 116, 104,
//             101, 32, 115, 97, 109, 101, 32, 110, 111, 110, 99, 101, 32, 116, 119, 105, 99, 101, 0,
//             0, 0, 0,
//         ],
//     });
//     let signature_length_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Signature length is incorrect".to_string(),
//         data: vec![
//             8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 83, 105, 103, 110, 97, 116, 117, 114, 101, 32,
//             108, 101, 110, 103, 116, 104, 32, 105, 115, 32, 105, 110, 99, 111, 114, 114, 101, 99,
//             116, 0, 0, 0,
//         ],
//     });
//     let v_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "v is neither 27 nor 28".to_string(),
//         data: vec![
//             8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 118, 32, 105, 115, 32, 110, 101, 105, 116, 104,
//             101, 114, 32, 50, 55, 32, 110, 111, 114, 32, 50, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//         ],
//     });
//     let signature_is_incorrect = TxRevertReason::ValidationFailed(VmRevertReason::General {
//         msg: "Account validation returned invalid magic value. Most often this means that the signature is incorrect".to_string(),
//         data: vec![],
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
//             gas_limit: U256::from(70000000u32),
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
//                 gas_limit: U256::from(100000000u32),
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
// fn insert_contracts(raw_storage: &mut InMemoryStorage, contracts: Vec<(DeployedContract, bool)>) {
//     for (contract, is_account) in contracts {
//         let deployer_code_key = get_code_key(contract.account_id.address());
//         raw_storage.set_value(deployer_code_key, hash_bytecode(&contract.bytecode));

//         if is_account {
//             let is_account_key = get_is_account_key(contract.account_id.address());
//             raw_storage.set_value(is_account_key, u256_to_h256(1_u32.into()));
//         }

//         raw_storage.store_factory_dep(hash_bytecode(&contract.bytecode), contract.bytecode);
//     }
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

// fn run_vm_with_raw_tx<'a, H: HistoryMode>(
//     oracle_tools: &'a mut OracleTools<'a, false, H>,
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

//     let block_gas_price_per_pubdata = block_context.context.block_gas_price_per_pubdata();

//     let overhead = tx.overhead_gas(block_gas_price_per_pubdata as u32);
//     push_raw_transaction_to_bootloader_memory(
//         &mut vm,
//         tx,
//         TxExecutionMode::VerifyExecute,
//         overhead,
//         None,
//     );
//     let VmBlockResult {
//         full_result: result,
//         ..
//     } = vm.execute_till_block_end(BootloaderJobType::TransactionExecution);

//     (result, tx_has_failed(&vm.state, 0))
// }

// #[test]
// fn test_nonce_holder() {
//     let account_address = H160::random();
//     let mut vm_test_env =
//         VmTestEnv::new_with_contracts(&[(account_address, read_nonce_holder_tester())]);

//     vm_test_env.set_rich_account(&account_address);

//     let mut run_nonce_test = |nonce: U256,
//                               test_mode: NonceHolderTestMode,
//                               error_message: Option<String>,
//                               comment: &'static str| {
//         let tx = get_nonce_holder_test_tx(
//             nonce,
//             account_address,
//             test_mode,
//             &vm_test_env.block_context,
//         );

//         let (result, tx_has_failed) = vm_test_env.run_vm(tx);
//         if let Some(msg) = error_message {
//             let expected_error =
//                 TxRevertReason::ValidationFailed(VmRevertReason::General { msg, data: vec![] });
//             assert_eq!(
//                 result
//                     .revert_reason
//                     .expect("No revert reason")
//                     .revert_reason
//                     .to_string(),
//                 expected_error.to_string(),
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
//     let mut vm_test_env = VmTestEnv::default();
//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
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
//             key: u256_to_h256(U256::from(vm_helper.block_context.context.block_timestamp)),
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

//     vm_helper.oracle_tools.decommittment_processor.populate(
//         vec![(
//             h256_to_u256(contract_code_hash),
//             bytes_to_be_words(contract_code),
//         )],
//         Timestamp(0),
//     );

//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(
//         &mut vm,
//         &l1_deploy_tx,
//         TxExecutionMode::VerifyExecute,
//         None,
//     );

//     let res = vm.execute_next_tx(u32::MAX, false).unwrap();

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
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     let res = StorageWritesDeduplicator::apply_on_empty_state(
//         &vm.execute_next_tx(u32::MAX, false)
//             .unwrap()
//             .result
//             .logs
//             .storage_logs,
//     );
//     assert_eq!(res.initial_storage_writes, 0);

//     let tx = get_l1_execute_test_contract_tx(deployed_address, false);
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);
//     let res = StorageWritesDeduplicator::apply_on_empty_state(
//         &vm.execute_next_tx(u32::MAX, false)
//             .unwrap()
//             .result
//             .logs
//             .storage_logs,
//     );
//     assert_eq!(res.initial_storage_writes, 2);

//     let repeated_writes = res.repeated_storage_writes;

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);
//     let res = StorageWritesDeduplicator::apply_on_empty_state(
//         &vm.execute_next_tx(u32::MAX, false)
//             .unwrap()
//             .result
//             .logs
//             .storage_logs,
//     );
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
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);
//     let execution_result = vm.execute_next_tx(u32::MAX, false).unwrap();
//     // The method is not payable, so the transaction with non-zero value should fail
//     assert_eq!(
//         execution_result.status,
//         TxExecutionStatus::Failure,
//         "The transaction should fail"
//     );

//     let res =
//         StorageWritesDeduplicator::apply_on_empty_state(&execution_result.result.logs.storage_logs);

//     // There are 2 initial writes here:
//     // - totalSupply of ETH token
//     // - balance of the refund recipient
//     assert_eq!(res.initial_storage_writes, 2);
// }

// #[test]
// fn test_invalid_bytecode() {
//     let mut vm_test_env = VmTestEnv::default();

//     let block_gas_per_pubdata = vm_test_env
//         .block_context
//         .context
//         .block_gas_price_per_pubdata();

//     let mut test_vm_with_custom_bytecode_hash =
//         |bytecode_hash: H256, expected_revert_reason: Option<TxRevertReason>| {
//             let mut oracle_tools =
//                 OracleTools::new(vm_test_env.storage_ptr.as_mut(), HistoryEnabled);

//             let (encoded_tx, predefined_overhead) = get_l1_tx_with_custom_bytecode_hash(
//                 h256_to_u256(bytecode_hash),
//                 block_gas_per_pubdata as u32,
//             );

//             run_vm_with_custom_factory_deps(
//                 &mut oracle_tools,
//                 vm_test_env.block_context.context,
//                 &vm_test_env.block_properties,
//                 encoded_tx,
//                 predefined_overhead,
//                 expected_revert_reason,
//             );
//         };

//     let failed_to_mark_factory_deps = |msg: &str, data: Vec<u8>| {
//         TxRevertReason::FailedToMarkFactoryDependencies(VmRevertReason::General {
//             msg: msg.to_string(),
//             data,
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
//             vec![
//                 8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 67, 111, 100, 101, 32, 108, 101, 110,
//                 103, 116, 104, 32, 105, 110, 32, 119, 111, 114, 100, 115, 32, 109, 117, 115, 116,
//                 32, 98, 101, 32, 111, 100, 100,
//             ],
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
//             vec![
//                 8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 73, 110, 99, 111, 114, 114, 101, 99,
//                 116, 108, 121, 32, 102, 111, 114, 109, 97, 116, 116, 101, 100, 32, 98, 121, 116,
//                 101, 99, 111, 100, 101, 72, 97, 115, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             ],
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
//             vec![
//                 8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 73, 110, 99, 111, 114, 114, 101, 99,
//                 116, 108, 121, 32, 102, 111, 114, 109, 97, 116, 116, 101, 100, 32, 98, 121, 116,
//                 101, 99, 111, 100, 101, 72, 97, 115, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             ],
//         )),
//     );
// }

// #[test]
// fn test_tracing_of_execution_errors() {
//     // In this test, we are checking that the execution errors are transmitted correctly from the bootloader.
//     let contract_address = Address::random();

//     let mut vm_test_env =
//         VmTestEnv::new_with_contracts(&[(contract_address, read_error_contract())]);

//     let private_key = H256::random();

//     let tx = get_error_tx(
//         private_key,
//         Nonce(0),
//         contract_address,
//         Fee {
//             gas_limit: U256::from(1000000u32),
//             max_fee_per_gas: U256::from(10000000000u64),
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     );

//     vm_test_env.set_rich_account(&tx.common_data.initiator_address);
//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(
//         &mut vm,
//         &tx.into(),
//         TxExecutionMode::VerifyExecute,
//         None,
//     );

//     let mut tracer = TransactionResultTracer::new(usize::MAX, false);
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
//                     msg: "short".to_string(),
//                     data: vec![
//                         8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                         0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                         0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 115, 104, 111,
//                         114, 116, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                         0, 0, 0, 0, 0
//                     ],
//                 }
//             )
//         }
//         _ => panic!(
//             "Tracer captured incorrect result {:#?}",
//             tracer.revert_reason
//         ),
//     }

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();
//     let tx = get_error_tx(
//         private_key,
//         Nonce(1),
//         contract_address,
//         Fee {
//             gas_limit: U256::from(1000000u32),
//             max_fee_per_gas: U256::from(10000000000u64),
//             max_priority_fee_per_gas: U256::zero(),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     );
//     push_transaction_to_bootloader_memory(
//         &mut vm,
//         &tx.into(),
//         TxExecutionMode::VerifyExecute,
//         None,
//     );

//     let mut tracer = TransactionResultTracer::new(10, false);
//     assert_eq!(
//         vm.execute_with_custom_tracer(&mut tracer),
//         VmExecutionStopReason::TracerRequestedStop,
//     );
//     assert!(tracer.is_limit_reached());
// }

// /// Checks that `TX_GAS_LIMIT_OFFSET` constant is correct.
// #[test]
// fn test_tx_gas_limit_offset() {
//     let gas_limit = U256::from(999999);
//     let mut vm_test_env = VmTestEnv::default();

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

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

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
//     let mut vm_test_env = VmTestEnv::default();

//     let base_fee = vm_test_env.block_context.base_fee;
//     let account_pk = H256::random();
//     let contract_code = read_test_contract();
//     let tx: Transaction = get_deploy_tx(
//         account_pk,
//         Nonce(0),
//         &contract_code,
//         vec![],
//         &[],
//         Fee {
//             gas_limit: U256::from(20000000u32),
//             max_fee_per_gas: U256::from(base_fee),
//             max_priority_fee_per_gas: U256::from(0),
//             gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
//         },
//     )
//     .into();

//     let sender_address = tx.initiator_account();
//     let nonce_key = get_nonce_key(&sender_address);

//     // Check that the next write to the nonce key will be initial.
//     assert!(vm_test_env.storage_ptr.is_write_initial(&nonce_key));

//     // Set balance to be able to pay fee for txs.
//     vm_test_env.set_rich_account(&sender_address);

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     vm.execute_next_tx(u32::MAX, false)
//         .expect("Bootloader failed while processing the first transaction");
//     // Check that `is_write_initial` still returns true for the nonce key.
//     assert!(vm_test_env.storage_ptr.is_write_initial(&nonce_key));
// }

// pub fn get_l1_tx_with_custom_bytecode_hash(
//     bytecode_hash: U256,
//     block_gas_per_pubdata: u32,
// ) -> (Vec<U256>, u32) {
//     let tx: TransactionData = get_l1_execute_test_contract_tx(Default::default(), false).into();
//     let predefined_overhead =
//         tx.overhead_gas_with_custom_factory_deps(vec![bytecode_hash], block_gas_per_pubdata);
//     let tx_bytes = tx.abi_encode_with_custom_factory_deps(vec![bytecode_hash]);

//     (bytes_to_be_words(tx_bytes), predefined_overhead)
// }

// pub fn get_l1_execute_test_contract_tx(deployed_address: Address, with_panic: bool) -> Transaction {
//     let sender = H160::random();
//     get_l1_execute_test_contract_tx_with_sender(
//         sender,
//         deployed_address,
//         with_panic,
//         U256::zero(),
//         false,
//     )
// }

// pub fn get_l1_tx_with_large_output(sender: Address, deployed_address: Address) -> Transaction {
//     let test_contract = load_contract(
//         "etc/contracts-test-data/artifacts-zk/contracts/long-return-data/long-return-data.sol/LongReturnData.json",
//     );

//     let function = test_contract.function("longReturnData").unwrap();

//     let calldata = function
//         .encode_input(&[])
//         .expect("failed to encode parameters");

//     Transaction {
//         common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
//             sender,
//             gas_limit: U256::from(100000000u32),
//             gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
//             ..Default::default()
//         }),
//         execute: Execute {
//             contract_address: deployed_address,
//             calldata,
//             value: U256::zero(),
//             factory_deps: None,
//         },
//         received_timestamp_ms: 0,
//     }
// }

// #[test]
// fn test_call_tracer() {
//     let mut vm_test_env = VmTestEnv::default();

//     let sender = H160::random();

//     let contract_code = read_test_contract();
//     let contract_code_hash = hash_bytecode(&contract_code);
//     let l1_deploy_tx = get_l1_deploy_tx(&contract_code, &[]);
//     let l1_deploy_tx_data: TransactionData = l1_deploy_tx.clone().into();

//     let sender_address_counter = l1_deploy_tx_data.from();

//     vm_test_env.set_rich_account(&sender_address_counter);
//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);

//     vm_helper.oracle_tools.decommittment_processor.populate(
//         vec![(
//             h256_to_u256(contract_code_hash),
//             bytes_to_be_words(contract_code),
//         )],
//         Timestamp(0),
//     );

//     let contract_code = read_long_return_data_contract();
//     let contract_code_hash = hash_bytecode(&contract_code);
//     let l1_deploy_long_return_data_tx = get_l1_deploy_tx(&contract_code, &[]);
//     vm_helper.oracle_tools.decommittment_processor.populate(
//         vec![(
//             h256_to_u256(contract_code_hash),
//             bytes_to_be_words(contract_code),
//         )],
//         Timestamp(0),
//     );

//     let tx_data: TransactionData = l1_deploy_long_return_data_tx.clone().into();
//     let sender_long_return_address = tx_data.from();
//     // The contract should be deployed successfully.
//     let deployed_address_long_return_data =
//         deployed_address_create(sender_long_return_address, U256::zero());
//     let mut vm = vm_helper.vm();

//     push_transaction_to_bootloader_memory(
//         &mut vm,
//         &l1_deploy_tx,
//         TxExecutionMode::VerifyExecute,
//         None,
//     );

//     // The contract should be deployed successfully.
//     let deployed_address = deployed_address_create(sender_address_counter, U256::zero());
//     let res = vm.execute_next_tx(u32::MAX, true).unwrap();
//     let calls = res.call_traces;
//     let mut create_call = None;
//     // The first MIMIC call is call to value simulator. All calls goes through it.
//     // The second MIMIC call is call to Deployer contract.
//     // And only third level call is construct call to the newly deployed contract And we call it create_call.
//     for call in &calls {
//         if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//             for call in &call.calls {
//                 if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//                     for call in &call.calls {
//                         if let CallType::Create = call.r#type {
//                             create_call = Some(call.clone());
//                         }
//                     }
//                 }
//             }
//         }
//     }
//     let expected = Call {
//         r#type: CallType::Create,
//         to: deployed_address,
//         from: sender_address_counter,
//         parent_gas: 0,
//         gas_used: 0,
//         gas: 0,
//         value: U256::zero(),
//         input: vec![],
//         output: vec![
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0, 0, 0,
//         ],
//         error: None,
//         revert_reason: None,
//         calls: vec![],
//     };
//     assert_eq!(create_call.unwrap(), expected);

//     push_transaction_to_bootloader_memory(
//         &mut vm,
//         &l1_deploy_long_return_data_tx,
//         TxExecutionMode::VerifyExecute,
//         None,
//     );

//     vm.execute_next_tx(u32::MAX, false).unwrap();

//     let tx = get_l1_execute_test_contract_tx_with_sender(
//         sender,
//         deployed_address,
//         false,
//         U256::from(1u8),
//         true,
//     );

//     let tx_data: TransactionData = tx.clone().into();
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     let res = vm.execute_next_tx(u32::MAX, true).unwrap();
//     let calls = res.call_traces;

//     // We don't want to compare gas used, because it's not fully deterministic.
//     let expected = Call {
//         r#type: CallType::Call(FarCallOpcode::Mimic),
//         to: deployed_address,
//         from: tx_data.from(),
//         parent_gas: 0,
//         gas_used: 0,
//         gas: 0,
//         value: U256::from(1),
//         input: tx_data.data,
//         output: vec![
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 1,
//         ],
//         error: None,
//         revert_reason: None,
//         calls: vec![],
//     };

//     // First loop filter out the bootloaders calls and
//     // the second loop filters out the calls msg value simulator calls
//     for call in calls {
//         if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//             for call in call.calls {
//                 if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//                     assert_eq!(expected, call);
//                 }
//             }
//         }
//     }

//     let tx = get_l1_execute_test_contract_tx_with_sender(
//         sender,
//         deployed_address,
//         true,
//         U256::from(1u8),
//         true,
//     );

//     let tx_data: TransactionData = tx.clone().into();
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     let res = vm.execute_next_tx(u32::MAX, true).unwrap();
//     let calls = res.call_traces;

//     let expected = Call {
//         r#type: CallType::Call(FarCallOpcode::Mimic),
//         to: deployed_address,
//         from: tx_data.from(),
//         parent_gas: 257030,
//         gas_used: 348,
//         gas: 253008,
//         value: U256::from(1u8),
//         input: tx_data.data,
//         output: vec![],
//         error: None,
//         revert_reason: Some("This method always reverts".to_string()),
//         calls: vec![],
//     };

//     for call in calls {
//         if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//             for call in call.calls {
//                 if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//                     assert_eq!(expected, call);
//                 }
//             }
//         }
//     }

//     let tx = get_l1_tx_with_large_output(sender, deployed_address_long_return_data);

//     let tx_data: TransactionData = tx.clone().into();
//     push_transaction_to_bootloader_memory(&mut vm, &tx, TxExecutionMode::VerifyExecute, None);

//     assert_ne!(deployed_address_long_return_data, deployed_address);
//     let res = vm.execute_next_tx(u32::MAX, true).unwrap();
//     let calls = res.call_traces;
//     for call in calls {
//         if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//             for call in call.calls {
//                 if let CallType::Call(FarCallOpcode::Mimic) = call.r#type {
//                     assert_eq!(call.input, tx_data.data);
//                     assert_eq!(
//                         call.revert_reason,
//                         Some("Unknown revert reason".to_string())
//                     );
//                 }
//             }
//         }
//     }
// }

// #[test]
// fn test_get_used_contracts() {
//     let mut vm_test_env = VmTestEnv::default();

//     let mut vm_helper = VmTestHelper::new(&mut vm_test_env);
//     let mut vm = vm_helper.vm();

//     assert!(known_bytecodes_without_aa_code(&vm).is_empty());

//     // create and push and execute some not-empty factory deps transaction with success status
//     // to check that get_used_contracts() updates
//     let contract_code = read_test_contract();
//     let contract_code_hash = hash_bytecode(&contract_code);
//     let tx1 = get_l1_deploy_tx(&contract_code, &[]);

//     push_transaction_to_bootloader_memory(&mut vm, &tx1, TxExecutionMode::VerifyExecute, None);

//     let res1 = vm.execute_next_tx(u32::MAX, true).unwrap();
//     assert_eq!(res1.status, TxExecutionStatus::Success);
//     assert!(vm
//         .get_used_contracts()
//         .contains(&h256_to_u256(contract_code_hash)));

//     assert_eq!(
//         vm.get_used_contracts()
//             .into_iter()
//             .collect::<HashSet<U256>>(),
//         known_bytecodes_without_aa_code(&vm)
//             .keys()
//             .cloned()
//             .collect::<HashSet<U256>>()
//     );

//     // create push and execute some non-empty factory deps transaction that fails
//     // (known_bytecodes will be updated but we expect get_used_contracts() to not be updated)

//     let mut tx2 = tx1;
//     tx2.execute.contract_address = L1_MESSENGER_ADDRESS;

//     let calldata = vec![1, 2, 3];
//     let big_calldata: Vec<u8> = calldata
//         .iter()
//         .cycle()
//         .take(calldata.len() * 1024)
//         .cloned()
//         .collect();

//     tx2.execute.calldata = big_calldata;
//     tx2.execute.factory_deps = Some(vec![vec![1; 32]]);

//     push_transaction_to_bootloader_memory(&mut vm, &tx2, TxExecutionMode::VerifyExecute, None);

//     let res2 = vm.execute_next_tx(u32::MAX, false).unwrap();

//     assert_eq!(res2.status, TxExecutionStatus::Failure);

//     for factory_dep in tx2.execute.factory_deps.unwrap() {
//         let hash = hash_bytecode(&factory_dep);
//         let hash_to_u256 = h256_to_u256(hash);
//         assert!(known_bytecodes_without_aa_code(&vm)
//             .keys()
//             .contains(&hash_to_u256));
//         assert!(!vm.get_used_contracts().contains(&hash_to_u256));
//     }
// }

// fn known_bytecodes_without_aa_code<H: HistoryMode>(vm: &VmInstance<H>) -> HashMap<U256, Vec<U256>> {
//     let mut known_bytecodes_without_aa_code = vm
//         .state
//         .decommittment_processor
//         .known_bytecodes
//         .inner()
//         .clone();

//     known_bytecodes_without_aa_code
//         .remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash))
//         .unwrap();

//     known_bytecodes_without_aa_code
// }

// #[tokio::test]
// /// This test deploys 'buggy' account abstraction code, and then tries accessing it both with legacy
// /// and EIP712 transactions.
// /// Currently we support both, but in the future, we should allow only EIP712 transactions to access the AA accounts.
// async fn test_require_eip712() {
//     // Use 3 accounts:
//     // - private_address - EOA account, where we have the key
//     // - account_address - AA account, where the contract is deployed
//     // - beneficiary - an EOA account, where we'll try to transfer the tokens.
//     let account_address = H160::random();

//     let (bytecode, contract) = read_many_owners_custom_account_contract();

//     let mut vm_test_env = VmTestEnv::new_with_contracts(&[(account_address, bytecode)]);

//     let beneficiary = H160::random();

//     assert_eq!(vm_test_env.get_eth_balance(&beneficiary), U256::from(0));

//     let private_key = H256::random();
//     let private_address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
//     let pk_signer = PrivateKeySigner::new(private_key);

//     vm_test_env.set_rich_account(&account_address);
//     vm_test_env.set_rich_account(&private_address);

//     let chain_id: u16 = 270;

//     // First, let's set the owners of the AA account to the private_address.
//     // (so that messages signed by private_address, are authorized to act on behalf of the AA account).
//     {
//         let set_owners_function = contract.function("setOwners").unwrap();
//         let encoded_input = set_owners_function
//             .encode_input(&[Token::Array(vec![Token::Address(private_address)])]);

//         // Create a legacy transaction to set the owners.
//         let raw_tx = TransactionParameters {
//             nonce: U256::from(0),
//             to: Some(account_address),
//             gas: U256::from(100000000),
//             gas_price: Some(U256::from(10000000)),
//             value: U256::from(0),
//             data: encoded_input.unwrap(),
//             chain_id: chain_id as u64,
//             transaction_type: None,
//             access_list: None,
//             max_fee_per_gas: U256::from(1000000000),
//             max_priority_fee_per_gas: U256::from(1000000000),
//         };
//         let txn = pk_signer.sign_transaction(raw_tx).await.unwrap();

//         let (txn_request, hash) = TransactionRequest::from_bytes(&txn, chain_id).unwrap();

//         let mut l2_tx: L2Tx = L2Tx::from_request(txn_request, 100000).unwrap();
//         l2_tx.set_input(txn, hash);
//         let transaction: Transaction = l2_tx.try_into().unwrap();
//         let transaction_data: TransactionData = transaction.try_into().unwrap();

//         vm_test_env.run_vm_or_die(transaction_data);
//     }

//     let private_account_balance = vm_test_env.get_eth_balance(&private_address);

//     // And now let's do the transfer from the 'account abstraction' to 'beneficiary' (using 'legacy' transaction).
//     // Normally this would not work - unless the operator is malicious.
//     {
//         let aa_raw_tx = TransactionParameters {
//             nonce: U256::from(0),
//             to: Some(beneficiary),
//             gas: U256::from(100000000),
//             gas_price: Some(U256::from(10000000)),
//             value: U256::from(888000088),
//             data: vec![],
//             chain_id: 270,
//             transaction_type: None,
//             access_list: None,
//             max_fee_per_gas: U256::from(1000000000),
//             max_priority_fee_per_gas: U256::from(1000000000),
//         };

//         let aa_txn = pk_signer.sign_transaction(aa_raw_tx).await.unwrap();

//         let (aa_txn_request, aa_hash) = TransactionRequest::from_bytes(&aa_txn, 270).unwrap();

//         let mut l2_tx: L2Tx = L2Tx::from_request(aa_txn_request, 100000).unwrap();
//         l2_tx.set_input(aa_txn, aa_hash);
//         // Pretend that operator is malicious and sets the initiator to the AA account.
//         l2_tx.common_data.initiator_address = account_address;

//         let transaction: Transaction = l2_tx.try_into().unwrap();

//         let transaction_data: TransactionData = transaction.try_into().unwrap();

//         vm_test_env.run_vm_or_die(transaction_data);
//         assert_eq!(
//             vm_test_env.get_eth_balance(&beneficiary),
//             U256::from(888000088)
//         );
//         // Make sure that the tokens were transfered from the AA account.
//         assert_eq!(
//             private_account_balance,
//             vm_test_env.get_eth_balance(&private_address)
//         )
//     }

//     // Now send the 'classic' EIP712 transaction
//     {
//         let tx_712 = L2Tx::new(
//             beneficiary,
//             vec![],
//             Nonce(1),
//             Fee {
//                 gas_limit: U256::from(1000000000),
//                 max_fee_per_gas: U256::from(1000000000),
//                 max_priority_fee_per_gas: U256::from(1000000000),
//                 gas_per_pubdata_limit: U256::from(1000000000),
//             },
//             account_address,
//             U256::from(28374938),
//             None,
//             Default::default(),
//         );

//         let transaction_request: TransactionRequest = tx_712.into();

//         let domain = Eip712Domain::new(L2ChainId(chain_id));
//         let signature = pk_signer
//             .sign_typed_data(&domain, &transaction_request)
//             .await
//             .unwrap();
//         let encoded_tx = transaction_request.get_signed_bytes(&signature, L2ChainId(chain_id));

//         let (aa_txn_request, aa_hash) =
//             TransactionRequest::from_bytes(&encoded_tx, chain_id).unwrap();

//         let mut l2_tx: L2Tx = L2Tx::from_request(aa_txn_request, 100000).unwrap();
//         l2_tx.set_input(encoded_tx, aa_hash);

//         let transaction: Transaction = l2_tx.try_into().unwrap();
//         let transaction_data: TransactionData = transaction.try_into().unwrap();

//         vm_test_env.run_vm_or_die(transaction_data);

//         assert_eq!(
//             vm_test_env.get_eth_balance(&beneficiary),
//             U256::from(916375026)
//         );
//         assert_eq!(
//             private_account_balance,
//             vm_test_env.get_eth_balance(&private_address)
//         );
//     }
// }

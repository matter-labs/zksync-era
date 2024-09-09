// use ethabi::Token;
// use itertools::Itertools;
// use zksync_types::{
//     get_immutable_key, get_l2_message_root_init_logs, AccountTreeId, StorageKey, StorageLog,
//     StorageLogKind, H256, IMMUTABLE_SIMULATOR_STORAGE_ADDRESS, L2_BRIDGEHUB_ADDRESS,
//     L2_MESSAGE_ROOT_ADDRESS,
// };

// use crate::{
//     // interface::{TxExecutionMode, VmExecutionMode, VmInterface},
//     vm_latest::{
//         tests::{
//             tester::{DeployContractsTx, TxType, VmTesterBuilder},
//             utils::read_message_root,
//         },
//         HistoryEnabled,
//     },
//     vm_m5::storage::Storage,
// };
/// Some of the constants of the system are implicitly calculated, but they may affect the code and so
/// we added additional checks on them to keep any unwanted changes of those apparent.
#[test]
fn test_that_bootloader_encoding_space_is_large_enoguh() {
    let encoding_space = crate::vm_latest::constants::get_bootloader_tx_encoding_space(
        crate::vm_latest::MultiVMSubversion::latest(),
    );
    assert!(encoding_space >= 330000, "Bootloader tx space is too small");
}

// Test that checks that the initial logs for the L2 Message Root are correct
// #[test]
// fn test_l2_message_root_init_logs() {
//     let mut vm = VmTesterBuilder::new(HistoryEnabled)
//         .with_empty_in_memory_storage()
//         .with_execution_mode(TxExecutionMode::VerifyExecute)
//         .with_random_rich_accounts(1)
//         .build();
//     let message_root_bytecode = read_message_root();
//     let account = &mut vm.rich_accounts[0];
//     let DeployContractsTx { tx, address, .. } = account.get_deploy_tx(
//         &message_root_bytecode,
//         Some(&[Token::Address(L2_BRIDGEHUB_ADDRESS)]),
//         TxType::L2,
//     );

//     vm.vm.push_transaction(tx);
//     let result = vm.vm.execute(VmExecutionMode::OneTx);
//     assert!(!result.result.is_failed(), "Transaction wasn't successful");

//     // That's the only key in the immutable simulator that should be changed. It depends on the address
//     // of the deployed contract, so we check that the way it was generated for a random deployed contract is the same.
//     let expected_change_immutable_key = get_immutable_key(&address, H256::zero());
//     let expected_genesis_immutable_key = get_immutable_key(&L2_MESSAGE_ROOT_ADDRESS, H256::zero());

//     let mut expected_init_logs = get_l2_message_root_init_logs()
//         .into_iter()
//         .map(|x| StorageLog {
//             // We unify all the logs to all have the same kind
//             kind: StorageLogKind::InitialWrite,
//             key: x.key,
//             value: x.value,
//         })
//         .collect::<Vec<_>>();

//     let ordering = |a: &StorageLog, b: &StorageLog| match a.key.cmp(&b.key) {
//         std::cmp::Ordering::Equal => a.value.cmp(&b.value),
//         other => other,
//     };

//     expected_init_logs.sort_by(ordering);

//     let correct_init_logs = vm
//         .vm
//         .storage
//         .borrow_mut()
//         .get_modified_storage_keys()
//         .iter()
//         .filter_map(|(&storage_key, &value)| {
//             if *storage_key.address() == address {
//                 Some(StorageLog {
//                     kind: StorageLogKind::InitialWrite,
//                     key: StorageKey::new(
//                         // Note, that it in the end we will compare those with the genesis logs that
//                         // have the `L2_MESSAGE_ROOT_ADDRESS` as the address
//                         AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS),
//                         *storage_key.key(),
//                     ),
//                     value,
//                 })
//             } else if *storage_key.address() == IMMUTABLE_SIMULATOR_STORAGE_ADDRESS {
//                 assert!(
//                     *storage_key.key() == expected_change_immutable_key,
//                     "Incorrect immutable key has been changed"
//                 );

//                 Some(StorageLog {
//                     kind: StorageLogKind::InitialWrite,
//                     key: StorageKey::new(
//                         AccountTreeId::new(IMMUTABLE_SIMULATOR_STORAGE_ADDRESS),
//                         // For comparison to work, we replace the immutable key with the one that is used for genesis
//                         expected_genesis_immutable_key,
//                     ),
//                     value,
//                 })
//             } else {
//                 None
//             }
//         })
//         .sorted_by(ordering)
//         .collect::<Vec<_>>();

//     assert_eq!(expected_init_logs, correct_init_logs);

//     let batch_result = vm.vm.execute(VmExecutionMode::Batch);
//     assert!(
//         !batch_result.result.is_failed(),
//         "Transaction wasn't successful"
//     );
// }

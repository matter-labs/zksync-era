use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_types::l2_to_l1_log::L2ToL1Log;
use zksync_types::storage_writes_deduplicator::StorageWritesDeduplicator;
use zksync_types::{get_code_key, get_known_code_key, U256};
use zksync_utils::u256_to_h256;

use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterface};
use crate::vm_latest::HistoryEnabled;
use crate::vm_virtual_blocks::tests::tester::{TxType, VmTesterBuilder};
use crate::vm_virtual_blocks::tests::utils::{
    read_test_contract, verify_required_storage, BASE_SYSTEM_CONTRACTS,
};
use crate::vm_virtual_blocks::types::internals::TransactionData;

#[test]
fn test_l1_tx_execution() {
    // In this test, we try to execute a contract deployment from L1
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->L2 communication, the same it would likely be done during the priority mode.

    // There are always at least 3 initial writes here, because we pay fees from l1:
    // - totalSupply of ETH token
    // - balance of the refund recipient
    // - balance of the bootloader
    // - tx_rollout hash

    let basic_initial_writes = 1;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let contract_code = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 1 });
    let tx_data: TransactionData = deploy_tx.tx.clone().into();

    let required_l2_to_l1_logs = vec![L2ToL1Log {
        shard_id: 0,
        is_service: true,
        tx_number_in_block: 0,
        sender: BOOTLOADER_ADDRESS,
        key: tx_data.tx_hash(0.into()),
        value: u256_to_h256(U256::from(1u32)),
    }];

    vm.vm.push_transaction(deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&deploy_tx.address);

    let expected_slots = vec![
        (u256_to_h256(U256::from(1u32)), known_codes_key),
        (deploy_tx.bytecode_hash, account_code_key),
    ];
    assert!(!res.result.is_failed());

    verify_required_storage(&vm.vm.state, expected_slots);

    assert_eq!(res.logs.l2_to_l1_logs, required_l2_to_l1_logs);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        true,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(VmExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);

    // Tx panicked
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 0);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx.clone());
    let res = vm.vm.execute(VmExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We changed one slot inside contract
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);

    // No repeated writes
    let repeated_writes = res.repeated_storage_writes;
    assert_eq!(res.repeated_storage_writes, 0);

    vm.vm.push_transaction(tx);
    let storage_logs = vm.vm.execute(VmExecutionMode::OneTx).logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We do the same storage write, it will be deduplicated, so still 4 initial write and 0 repeated
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);
    assert_eq!(res.repeated_storage_writes, repeated_writes);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        Some(10.into()),
        false,
        TxType::L1 { serial_id: 1 },
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    // Method is not payable tx should fail
    assert!(result.result.is_failed(), "The transaction should fail");

    let res = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    // There are only basic initial writes
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 2);
}

use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_system_constants::{BOOTLOADER_ADDRESS, L1_MESSENGER_ADDRESS};
use zksync_test_account::Account;
use zksync_types::{
    get_code_key, get_known_code_key,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    Execute, ExecuteTransactionCommon, K256PrivateKey, U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    utils::StorageWritesDeduplicator,
    vm_latest::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{read_test_contract, verify_required_storage, BASE_SYSTEM_CONTRACTS},
        },
        types::internals::TransactionData,
        HistoryEnabled,
    },
};

#[test]
fn test_l1_tx_execution() {
    // In this test, we try to execute a contract deployment from L1
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->L2 communication, the same it would likely be done during the priority mode.

    // There are always at least 9 initial writes here, because we pay fees from l1:
    // - `totalSupply` of ETH token
    // - balance of the refund recipient
    // - balance of the bootloader
    // - `tx_rolling` hash
    // - `gasPerPubdataByte`
    // - `basePubdataSpent`
    // - rolling hash of L2->L1 logs
    // - transaction number in block counter
    // - L2->L1 log counter in `L1Messenger`

    // TODO(PLA-537): right now we are using 5 slots instead of 9 due to 0 fee for transaction.
    let basic_initial_writes = 5;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let contract_code = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 0 });
    let tx_data: TransactionData = deploy_tx.tx.clone().into();

    let required_l2_to_l1_logs: Vec<_> = vec![L2ToL1Log {
        shard_id: 0,
        is_service: true,
        tx_number_in_block: 0,
        sender: BOOTLOADER_ADDRESS,
        key: tx_data.tx_hash(0.into()),
        value: u256_to_h256(U256::from(1u32)),
    }]
    .into_iter()
    .map(UserL2ToL1Log)
    .collect();

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

    assert_eq!(res.logs.user_l2_to_l1_logs, required_l2_to_l1_logs);

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
    assert_eq!(res.initial_storage_writes, basic_initial_writes);

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
    // We changed one slot inside contract. However, the rewrite of the `basePubdataSpent` didn't happen, since it was the same
    // as the start of the previous tx. Thus we have `+1` slot for the changed counter and `-1` slot for base pubdata spent
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 0);

    // No repeated writes
    let repeated_writes = res.repeated_storage_writes;
    assert_eq!(res.repeated_storage_writes, 0);

    vm.vm.push_transaction(tx);
    let storage_logs = vm.vm.execute(VmExecutionMode::OneTx).logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We do the same storage write, it will be deduplicated, so still 4 initial write and 0 repeated.
    // But now the base pubdata spent has changed too.
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
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);
}

#[test]
fn test_l1_tx_execution_high_gas_limit() {
    // In this test, we try to execute an L1->L2 transaction with a high gas limit.
    // Usually priority transactions with dangerously gas limit should even pass the checks on the L1,
    // however, they might pass during the transition period to the new fee model, so we check that we can safely process those.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![Account::new(
            K256PrivateKey::from_bytes([0xad; 32].into()).unwrap(),
        )])
        .build();

    let account = &mut vm.rich_accounts[0];

    let l1_messenger = l1_messenger_contract();

    let contract_function = l1_messenger.function("sendToL1").unwrap();
    let params = [
        // Even a message of size 100k should not be able to be sent by a priority transaction
        Token::Bytes(vec![0u8; 100_000]),
    ];
    let calldata = contract_function.encode_input(&params).unwrap();

    let mut tx = account.get_l1_tx(
        Execute {
            contract_address: L1_MESSENGER_ADDRESS,
            value: 0.into(),
            factory_deps: vec![],
            calldata,
        },
        0,
    );

    if let ExecuteTransactionCommon::L1(data) = &mut tx.common_data {
        // Using some large gas limit
        data.gas_limit = 300_000_000.into();
    } else {
        unreachable!()
    };

    vm.vm.push_transaction(tx);

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(res.result.is_failed(), "The transaction should've failed");
}

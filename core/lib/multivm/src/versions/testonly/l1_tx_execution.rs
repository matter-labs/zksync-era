use assert_matches::assert_matches;
use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_system_constants::{BOOTLOADER_ADDRESS, L1_MESSENGER_ADDRESS};
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{
    get_code_key, get_known_code_key, h256_to_u256,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    u256_to_h256, Address, Execute, ExecuteTransactionCommon, U256,
};

use super::{tester::VmTesterBuilder, ContractToDeploy, TestedVm, BASE_SYSTEM_CONTRACTS};
use crate::{
    interface::{
        ExecutionResult, InspectExecutionMode, TxExecutionMode, VmInterfaceExt, VmRevertReason,
    },
    utils::StorageWritesDeduplicator,
};

pub(crate) fn test_l1_tx_execution<VM: TestedVm>() {
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

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(
        TestContract::counter().bytecode,
        None,
        TxType::L1 { serial_id: 1 },
    );
    let tx_hash = deploy_tx.tx.hash();

    let required_l2_to_l1_logs: Vec<_> = vec![L2ToL1Log {
        shard_id: 0,
        is_service: true,
        tx_number_in_block: 0,
        sender: BOOTLOADER_ADDRESS,
        key: tx_hash,
        value: u256_to_h256(U256::from(1u32)),
    }]
    .into_iter()
    .map(UserL2ToL1Log)
    .collect();

    vm.vm.push_transaction(deploy_tx.tx.clone());

    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&deploy_tx.address);

    assert!(!res.result.is_failed());

    vm.vm.verify_required_storage(&[
        (known_codes_key, U256::from(1)),
        (account_code_key, h256_to_u256(deploy_tx.bytecode_hash)),
    ]);
    assert_eq!(res.logs.user_l2_to_l1_logs, required_l2_to_l1_logs);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        true,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
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
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We changed one slot inside contract.
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);

    // No repeated writes
    let repeated_writes = res.repeated_storage_writes;
    assert_eq!(res.repeated_storage_writes, 0);

    vm.vm.push_transaction(tx);
    let storage_logs = vm.vm.execute(InspectExecutionMode::OneTx).logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We do the same storage write, it will be deduplicated, so still 4 initial write and 0 repeated.
    // But now the base pubdata spent has changed too.
    assert_eq!(res.initial_storage_writes, basic_initial_writes + 1);
    assert_eq!(res.repeated_storage_writes, repeated_writes);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        Some(10.into()),
        false,
        TxType::L1 { serial_id: 1 },
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    // Method is not payable tx should fail
    assert!(result.result.is_failed(), "The transaction should fail");

    let res = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    assert_eq!(res.initial_storage_writes, basic_initial_writes + 1);
    assert_eq!(res.repeated_storage_writes, 1);
}

pub(crate) fn test_l1_tx_execution_high_gas_limit<VM: TestedVm>() {
    // In this test, we try to execute an L1->L2 transaction with a high gas limit.
    // Usually priority transactions with dangerously gas limit should even pass the checks on the L1,
    // however, they might pass during the transition period to the new fee model, so we check that we can safely process those.

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

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
            contract_address: Some(L1_MESSENGER_ADDRESS),
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

    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(res.result.is_failed(), "The transaction should've failed");
}

pub(crate) fn test_l1_tx_execution_gas_estimation_with_low_gas<VM: TestedVm>() {
    let counter_contract = TestContract::counter().bytecode.to_vec();
    let counter_address = Address::repeat_byte(0x11);
    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::EstimateFee)
        .with_custom_contracts(vec![ContractToDeploy::new(
            counter_contract,
            counter_address,
        )])
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let mut tx = account.get_test_contract_transaction(
        counter_address,
        false,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    let ExecuteTransactionCommon::L1(data) = &mut tx.common_data else {
        unreachable!();
    };
    // This gas limit is chosen so that transaction starts getting executed by the bootloader, but then runs out of gas
    // before its execution result is posted.
    data.gas_limit = 15_000.into();

    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(
        &res.result,
        ExecutionResult::Revert { output: VmRevertReason::General { msg, .. } }
            if msg.contains("reverted with empty reason")
    );
}

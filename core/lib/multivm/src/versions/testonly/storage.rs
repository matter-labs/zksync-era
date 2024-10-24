use ethabi::Token;
use zksync_contracts::{load_contract, read_bytecode};
use zksync_types::{Address, Execute, U256};

use super::{tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt};

fn test_storage<VM: TestedVm>(first_tx_calldata: Vec<u8>, second_tx_calldata: Vec<u8>) -> u32 {
    let bytecode = read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    let test_contract_address = Address::repeat_byte(1);

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(bytecode, test_contract_address)])
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];

    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_contract_address),
            calldata: first_tx_calldata,
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_contract_address),
            calldata: second_tx_calldata,
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.make_snapshot();
    vm.vm.push_transaction(tx1);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "First tx failed");
    vm.vm.pop_snapshot_no_rollback();

    // We rollback once because transient storage and rollbacks are a tricky combination.
    vm.vm.make_snapshot();
    vm.vm.push_transaction(tx2.clone());
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Second tx failed");
    vm.vm.rollback_to_the_latest_snapshot();

    vm.vm.make_snapshot();
    vm.vm.push_transaction(tx2);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Second tx failed on second run");

    result.statistics.pubdata_published
}

fn test_storage_one_tx<VM: TestedVm>(second_tx_calldata: Vec<u8>) -> u32 {
    test_storage::<VM>(vec![], second_tx_calldata)
}

pub(crate) fn test_storage_behavior<VM: TestedVm>() {
    let contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    // In all of the tests below we provide the first tx to ensure that the tracers will not include
    // the statistics from the start of the bootloader and will only include those for the transaction itself.

    let base_pubdata = test_storage_one_tx::<VM>(vec![]);
    let simple_test_pubdata = test_storage_one_tx::<VM>(
        contract
            .function("simpleWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_pubdata = test_storage_one_tx::<VM>(
        contract
            .function("resettingWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_via_revert_pubdata = test_storage_one_tx::<VM>(
        contract
            .function("resettingWriteViaRevert")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );

    assert_eq!(simple_test_pubdata - base_pubdata, 65);
    assert_eq!(resetting_write_pubdata - base_pubdata, 34);
    assert_eq!(resetting_write_via_revert_pubdata - base_pubdata, 34);
}

pub(crate) fn test_transient_storage_behavior<VM: TestedVm>() {
    let contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    let first_tstore_test = contract
        .function("testTransientStore")
        .unwrap()
        .encode_input(&[])
        .unwrap();
    // Second transaction checks that, as expected, the transient storage is cleared after the first transaction.
    let second_tstore_test = contract
        .function("assertTValue")
        .unwrap()
        .encode_input(&[Token::Uint(U256::zero())])
        .unwrap();

    test_storage::<VM>(first_tstore_test, second_tstore_test);
}

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_test_contracts::{LoadnextContractExecutionParams, TestContract, TxType};
use zksync_types::{AccountTreeId, Address, Execute, StorageKey, H256, U256};

use super::{
    get_empty_storage, tester::VmTesterBuilder, ContractToDeploy, TestedVm,
    TestedVmWithStorageLimit, VmTester,
};
use crate::interface::{
    ExecutionResult, Halt, InspectExecutionMode, TxExecutionMode, VmInterfaceExt,
};

fn test_storage<VM: TestedVm>(first_tx_calldata: Vec<u8>, second_tx_calldata: Vec<u8>) -> u32 {
    let bytecode = TestContract::storage_test().bytecode.to_vec();
    let test_contract_address = Address::repeat_byte(1);

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
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
    let contract = TestContract::storage_test();

    // In all of the tests below we provide the first tx to ensure that the tracers will not include
    // the statistics from the start of the bootloader and will only include those for the transaction itself.

    let base_pubdata = test_storage_one_tx::<VM>(vec![]);
    let simple_test_pubdata =
        test_storage_one_tx::<VM>(contract.function("simpleWrite").encode_input(&[]).unwrap());
    let resetting_write_pubdata = test_storage_one_tx::<VM>(
        contract
            .function("resettingWrite")
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_via_revert_pubdata = test_storage_one_tx::<VM>(
        contract
            .function("resettingWriteViaRevert")
            .encode_input(&[])
            .unwrap(),
    );

    assert_eq!(simple_test_pubdata - base_pubdata, 65);
    assert_eq!(resetting_write_pubdata - base_pubdata, 34);
    assert_eq!(resetting_write_via_revert_pubdata - base_pubdata, 34);
}

pub(crate) fn test_transient_storage_behavior<VM: TestedVm>() {
    let contract = TestContract::storage_test();

    let first_tstore_test = contract
        .function("testTransientStore")
        .encode_input(&[])
        .unwrap();
    // Second transaction checks that, as expected, the transient storage is cleared after the first transaction.
    let second_tstore_test = contract
        .function("assertTValue")
        .encode_input(&[Token::Uint(U256::zero())])
        .unwrap();

    test_storage::<VM>(first_tstore_test, second_tstore_test);
}

pub(crate) fn test_limiting_storage_writes<VM: TestedVmWithStorageLimit>(should_stop: bool) {
    let bytecode = TestContract::expensive().bytecode.to_vec();
    let test_address = Address::repeat_byte(1);

    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(bytecode, test_address)])
        .build();

    let account = &mut vm.rich_accounts[0];
    let test_fn = TestContract::expensive().function("expensive");
    let (executed_writes, limit) = if should_stop {
        (1_000, 100)
    } else {
        (100, 1_000)
    };
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: test_fn
                .encode_input(&[Token::Uint(executed_writes.into())])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute_with_storage_limit(limit);
    if should_stop {
        assert_matches!(
            &result.result,
            ExecutionResult::Halt {
                reason: Halt::TracerCustom(_)
            }
        );
    } else {
        assert!(!result.result.is_failed(), "{:?}", result.result);
    }
}

pub(crate) fn test_limiting_storage_reads<VM: TestedVmWithStorageLimit>(should_stop: bool) {
    let bytecode = TestContract::load_test().bytecode.to_vec();
    let test_address = Address::repeat_byte(1);
    let (executed_reads, limit) = if should_stop {
        (1_000, 100)
    } else {
        (100, 1_000)
    };
    let mut storage = get_empty_storage();
    // Set the read array length in the load test contract.
    storage.set_value(
        StorageKey::new(AccountTreeId::new(test_address), H256::zero()),
        H256::from_low_u64_be(executed_reads),
    );

    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(bytecode, test_address)])
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_loadnext_transaction(
        test_address,
        LoadnextContractExecutionParams {
            reads: executed_reads as usize,
            ..LoadnextContractExecutionParams::empty()
        },
        TxType::L2,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute_with_storage_limit(limit);
    if should_stop {
        assert_matches!(
            &result.result,
            ExecutionResult::Halt {
                reason: Halt::TracerCustom(_)
            }
        );
    } else {
        assert!(!result.result.is_failed(), "{:?}", result.result);
    }
}

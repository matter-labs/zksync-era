use crate::{
    era_vm::tests::tester::{TxType, VmTesterBuilder},
    interface::{ExecutionResult, VmExecutionMode, VmInterface},
};

#[test]
fn estimate_fee() {
    let mut vm_tester = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_deployer()
        .with_random_rich_accounts(1)
        .build();

    vm_tester.deploy_test_contract();
    let account = &mut vm_tester.rich_accounts[0];

    let tx = account.get_test_contract_transaction(
        vm_tester.test_contract.unwrap(),
        false,
        Default::default(),
        false,
        TxType::L2,
    );

    vm_tester.vm.push_transaction(tx);

    let result = vm_tester.vm.execute(VmExecutionMode::OneTx);
    assert!(matches!(result.result, ExecutionResult::Success { .. }));
}

#[test]
fn simple_execute() {
    let mut vm_tester = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_deployer()
        .with_random_rich_accounts(1)
        .build();

    vm_tester.deploy_test_contract();

    let account = &mut vm_tester.rich_accounts[0];

    let tx1 = account.get_test_contract_transaction(
        vm_tester.test_contract.unwrap(),
        false,
        Default::default(),
        false,
        TxType::L1 { serial_id: 1 },
    );

    let tx2 = account.get_test_contract_transaction(
        vm_tester.test_contract.unwrap(),
        true,
        Default::default(),
        false,
        TxType::L1 { serial_id: 1 },
    );

    let tx3 = account.get_test_contract_transaction(
        vm_tester.test_contract.unwrap(),
        false,
        Default::default(),
        false,
        TxType::L1 { serial_id: 1 },
    );
    let vm = &mut vm_tester.vm;
    vm.push_transaction(tx1);
    vm.push_transaction(tx2);
    vm.push_transaction(tx3);
    let tx = vm.execute(VmExecutionMode::OneTx);
    assert!(matches!(tx.result, ExecutionResult::Success { .. }));
    let tx = vm.execute(VmExecutionMode::OneTx);
    assert!(matches!(tx.result, ExecutionResult::Revert { .. }));
    let tx = vm.execute(VmExecutionMode::OneTx);
    assert!(matches!(tx.result, ExecutionResult::Success { .. }));
    let block_tip = vm.execute(VmExecutionMode::Batch);
    println!("BLOCK TIP RESULT {:?}", block_tip);
    assert!(matches!(block_tip.result, ExecutionResult::Success { .. }));
}

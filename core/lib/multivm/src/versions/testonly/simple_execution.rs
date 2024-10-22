use assert_matches::assert_matches;
use zksync_test_account::TxType;

use super::{default_pubdata_builder, tester::VmTesterBuilder, TestedVm};
use crate::interface::{ExecutionResult, InspectExecutionMode, VmInterfaceExt};

pub(crate) fn test_estimate_fee<VM: TestedVm>() {
    let mut vm_tester = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .build::<VM>();

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

    let result = vm_tester.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(result.result, ExecutionResult::Success { .. });
}

pub(crate) fn test_simple_execute<VM: TestedVm>() {
    let mut vm_tester = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .build::<VM>();

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
    let tx = vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(tx.result, ExecutionResult::Success { .. });
    let tx = vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(tx.result, ExecutionResult::Revert { .. });
    let tx = vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(tx.result, ExecutionResult::Success { .. });
    let block_tip = vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    assert_matches!(block_tip.result, ExecutionResult::Success { .. });
}

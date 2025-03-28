use assert_matches::assert_matches;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{Execute, H256};

use super::{default_pubdata_builder, extract_deploy_events, tester::VmTesterBuilder, TestedVm};
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

// TODO: also test EVM contract addresses once EVM emulator is implemented
pub(crate) fn test_create2_deployment_address<VM: TestedVm>() {
    let mut vm_tester = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_rich_accounts(1)
        .build::<VM>();
    let account = &mut vm_tester.rich_accounts[0];

    let (execute, deploy_params) =
        Execute::for_create2_deploy(H256::zero(), TestContract::counter().bytecode.to_vec(), &[]);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let expected_address = deploy_params.derive_address(account.address);

    vm_tester.vm.push_transaction(deploy_tx);
    let res = vm_tester.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(res.result, ExecutionResult::Success { .. });

    let deploy_events = extract_deploy_events(&res.logs.events);
    assert_eq!(deploy_events.len(), 1);
    assert_eq!(deploy_events[0], (account.address, expected_address));

    // Test with non-trivial salt and constructor args
    let (execute, deploy_params) = Execute::for_create2_deploy(
        H256::repeat_byte(1),
        TestContract::load_test().bytecode.to_vec(),
        &[ethabi::Token::Uint(100.into())],
    );
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let expected_address = deploy_params.derive_address(account.address);

    vm_tester.vm.push_transaction(deploy_tx);
    let res = vm_tester.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(res.result, ExecutionResult::Success { .. });

    let deploy_events = extract_deploy_events(&res.logs.events);
    assert_eq!(deploy_events.len(), 1);
    assert_eq!(deploy_events[0], (account.address, expected_address));
}

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{utils::deployed_address_create, Address, Execute, H256};

use super::{
    default_pubdata_builder, execute_oneshot_dump, extract_deploy_events, load_vm_dump,
    tester::VmTesterBuilder, ContractToDeploy, TestedVm,
};
use crate::interface::{
    storage::{StorageSnapshot, StorageView},
    ExecutionResult, InspectExecutionMode, VmFactory, VmInterfaceExt, VmRevertReason,
};

pub(crate) fn test_estimate_fee<VM: TestedVm>() {
    let mut vm_tester = VmTesterBuilder::new().with_rich_accounts(1).build::<VM>();

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
    let mut vm_tester = VmTesterBuilder::new().with_rich_accounts(1).build::<VM>();

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

pub(crate) fn test_create2_deployment_address<VM: TestedVm>() {
    let mut vm_tester = VmTesterBuilder::new().with_rich_accounts(1).build::<VM>();
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

pub(crate) fn test_reusing_create_address<VM: TestedVm>() {
    let factory_bytecode = TestContract::counter_factory().bytecode.to_vec();
    let factory_address = Address::repeat_byte(1);
    let mut vm_tester = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            factory_bytecode,
            factory_address,
        )])
        .build::<VM>();
    let account = &mut vm_tester.rich_accounts[0];

    let test_fn = TestContract::counter_factory().function("testReusingCreateAddress");
    let expected_address = deployed_address_create(factory_address, 0.into());
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(factory_address),
            calldata: test_fn
                .encode_input(&[Token::Address(expected_address)])
                .unwrap(),
            value: 0.into(),
            factory_deps: TestContract::counter_factory().factory_deps(),
        },
        None,
    );

    vm_tester.vm.push_transaction(test_tx);
    let res = vm_tester.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(res.result, ExecutionResult::Success { .. });
}

pub(crate) fn test_reusing_create2_salt<VM: TestedVm>() {
    let factory_bytecode = TestContract::counter_factory().bytecode.to_vec();
    let factory_address = Address::repeat_byte(1);
    let mut vm_tester = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            factory_bytecode,
            factory_address,
        )])
        .build::<VM>();
    let account = &mut vm_tester.rich_accounts[0];

    let test_fn = TestContract::counter_factory().function("testReusingCreate2Salt");
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(factory_address),
            calldata: test_fn.encode_input(&[]).unwrap(),
            value: 0.into(),
            factory_deps: TestContract::counter_factory().factory_deps(),
        },
        None,
    );

    vm_tester.vm.push_transaction(test_tx);
    let res = vm_tester.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(res.result, ExecutionResult::Success { .. });
}

pub(crate) fn test_transfer_to_self_with_low_gas_limit<VM>()
where
    VM: VmFactory<StorageView<StorageSnapshot>>,
{
    let dump = load_vm_dump("estimate_fee_for_transfer_to_self");
    let result = execute_oneshot_dump::<VM>(dump);
    assert_eq!(
        result.result,
        ExecutionResult::Revert {
            output: VmRevertReason::from(&[] as &[u8]),
        }
    );
}

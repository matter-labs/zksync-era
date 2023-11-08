use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterface};
use crate::tracers::CallTracer;
use crate::vm_latest::HistoryEnabled;
use crate::vm_virtual_blocks::constants::BLOCK_GAS_LIMIT;
use crate::vm_virtual_blocks::tests::tester::VmTesterBuilder;
use crate::vm_virtual_blocks::tests::utils::{read_max_depth_contract, read_test_contract};
use crate::vm_virtual_blocks::tracers::traits::ToTracerPointer;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use zksync_types::{Address, Execute};

// This test is ultra slow, so it's ignored by default.
#[test]
#[ignore]
fn test_max_depth() {
    let contarct = read_max_depth_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![(contarct, address, true)])
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: vec![],
            value: Default::default(),
            factory_deps: None,
        },
        None,
    );

    let result = Arc::new(OnceCell::new());
    let call_tracer = CallTracer::new(result.clone()).into_tracer_pointer();
    vm.vm.push_transaction(tx);
    let res = vm.vm.inspect(call_tracer.into(), VmExecutionMode::OneTx);
    assert!(result.get().is_some());
    assert!(res.result.is_failed());
}

#[test]
fn test_basic_behavior() {
    let contarct = read_test_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![(contarct, address, true)])
        .build();

    let increment_by_6_calldata =
        "7cf5dab00000000000000000000000000000000000000000000000000000000000000006";

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: address,
            calldata: hex::decode(increment_by_6_calldata).unwrap(),
            value: Default::default(),
            factory_deps: None,
        },
        None,
    );

    let result = Arc::new(OnceCell::new());
    let call_tracer = CallTracer::new(result.clone());
    vm.vm.push_transaction(tx);
    let res = vm.vm.inspect(
        call_tracer.into_tracer_pointer().into(),
        VmExecutionMode::OneTx,
    );

    let call_tracer_result = result.get().unwrap();

    assert_eq!(call_tracer_result.len(), 1);
    // Expect that there are a plenty of subcalls underneath.
    let subcall = &call_tracer_result[0].calls;
    assert!(subcall.len() > 10);
    assert!(!res.result.is_failed());
}

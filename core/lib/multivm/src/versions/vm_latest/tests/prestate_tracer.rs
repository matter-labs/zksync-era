use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_test_account::TxType;

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder, HistoryEnabled, ToTracerPointer,
    },
};

#[test]
fn test_prestate_tracer() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    vm.deploy_test_contract();
    let account = &mut vm.rich_accounts[0];

    let tx1 = account.get_test_contract_transaction(
        vm.test_contract.unwrap(),
        false,
        Default::default(),
        true,
        TxType::L2,
    );
    vm.vm.push_transaction(tx1);
    let contract_address = vm.test_contract.unwrap();
    let prestate_tracer_result = Arc::new(OnceCell::default());
    let prestate_tracer = crate::vm_latest::tracers::prestate_tracer::PrestateTracer::new(
        false,
        prestate_tracer_result.clone(),
    );
    let tracer_ptr = prestate_tracer.into_tracer_pointer();
    vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::Batch);

    let prestate_result = Arc::try_unwrap(prestate_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();

    assert!(prestate_result.1.contains_key(&contract_address));
}

#[test]
fn test_prestate_tracer_diff_mode() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_random_rich_accounts(1)
        .with_deployer()
        .with_gas_limit(BLOCK_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    vm.deploy_test_contract();
    let account = &mut vm.rich_accounts[0];

    let tx1 = account.get_test_contract_transaction(
        vm.test_contract.unwrap(),
        false,
        Default::default(),
        true,
        TxType::L2,
    );
    vm.vm.push_transaction(tx1);
    let prestate_tracer_result = Arc::new(OnceCell::default());
    let prestate_tracer = crate::vm_latest::tracers::prestate_tracer::PrestateTracer::new(
        true,
        prestate_tracer_result.clone(),
    );
    let tracer_ptr = prestate_tracer.into_tracer_pointer();
    vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::Batch);

    let prestate_result = Arc::try_unwrap(prestate_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();

    assert!(!prestate_result.0.is_empty());
    assert!(!prestate_result.1.is_empty());
}

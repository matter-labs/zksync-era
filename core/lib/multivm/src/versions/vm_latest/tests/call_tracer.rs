use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_types::{Address, Execute};

use super::TestedLatestVm;
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterface},
    tracers::CallTracer,
    versions::testonly::{call_tracer, read_max_depth_contract, ContractToDeploy, VmTesterBuilder},
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, ToTracerPointer, Vm},
};

// This test is ultra slow, so it's ignored by default.
#[test]
#[ignore]
fn test_max_depth() {
    let contarct = read_max_depth_contract();
    let address = Address::random();
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::account(contarct, address)])
        .build::<TestedLatestVm>();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: vec![],
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    let result = Arc::new(OnceCell::new());
    let call_tracer = CallTracer::new(result.clone()).into_tracer_pointer();
    vm.vm.push_transaction(tx);
    let res = vm
        .vm
        .inspect(&mut call_tracer.into(), InspectExecutionMode::OneTx);
    assert!(result.get().is_some());
    assert!(res.result.is_failed());
}

#[test]
fn basic_behavior() {
    call_tracer::test_basic_behavior::<Vm<_, _>>();
}

#[test]
fn transfer() {
    call_tracer::test_transfer::<Vm<_, _>>();
}

#[test]
fn reverted_tx() {
    call_tracer::test_reverted_tx::<Vm<_, _>>();
}

#[test]
fn reverted_deployment() {
    call_tracer::test_reverted_deployment_tx::<Vm<_, _>>();
}

#[test]
fn out_of_gas() {
    call_tracer::test_out_of_gas::<Vm<_, _>>();
}

#[test]
fn recursive_tx() {
    call_tracer::test_recursive_tx::<Vm<_, _>>();
}

#[test]
fn evm_to_eravm_call() {
    call_tracer::test_evm_to_eravm_call::<Vm<_, _>>();
}

#[test]
fn evm_deployment_tx() {
    call_tracer::test_evm_deployment_tx::<Vm<_, _>>();
}

#[test]
fn evm_deployment_from_contract() {
    call_tracer::test_evm_deployment_from_contract::<Vm<_, _>>();
}

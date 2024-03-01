use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_test_account::TxType;
use zksync_types::H256;

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder, tracers::PrestateTracer,
        HistoryEnabled, ToTracerPointer,
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
    let prestate_tracer = PrestateTracer::new(false, prestate_tracer_result.clone());
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
    let contract_address = vm.test_contract.unwrap();
    let prestate_tracer_result = Arc::new(OnceCell::default());
    let prestate_tracer = PrestateTracer::new(true, prestate_tracer_result.clone());
    let tracer_ptr = prestate_tracer.into_tracer_pointer();
    vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::Batch);

    let prestate_result = Arc::try_unwrap(prestate_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();

    assert!(prestate_result.0.len() > 0);
    assert!(prestate_result.1.len() > 0);
    let contract_address_account = prestate_result.0.get(&contract_address).unwrap().clone();
    let binding = contract_address_account.storage.unwrap();
    let contract_storage_value_pre_tx = binding.get(&H256::zero()).unwrap();
    let contract_address_account_post = prestate_result.1.get(&contract_address).unwrap().clone();
    let binding = contract_address_account_post.storage.unwrap();
    let contract_storage_value_post_tx = binding.get(&H256::zero()).unwrap();
    assert!(contract_storage_value_pre_tx < contract_storage_value_post_tx);
}

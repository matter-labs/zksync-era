use std::borrow::Borrow;

use crate::vm_latest::ToTracerPointer;
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::BLOCK_GAS_LIMIT, tests::tester::VmTesterBuilder, tracers::PrestateTracer,
        HistoryEnabled,
    },
};
use zksync_test_account::TxType;
use zksync_types::{Address, Execute, U256};

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

    println!("tx: {:?}", vm.test_contract.unwrap());
    vm.vm.push_transaction(tx1);

    println!("account: {:?}", account);
    let prestate_tracer = PrestateTracer::new();
    let tracer_ptr = prestate_tracer.into_tracer_pointer();
    let res = vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::Batch);
    assert!(1 == 0);
}

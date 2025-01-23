use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{utils::deployed_address_create, Execute, U256};

use super::TestedLatestVm;
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterface, VmInterfaceExt},
    tracers::PrestateTracer,
    versions::testonly::VmTesterBuilder,
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, ToTracerPointer},
};

#[test]
fn test_prestate_tracer() {
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<TestedLatestVm>();

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
    vm.vm
        .inspect(&mut tracer_ptr.into(), InspectExecutionMode::OneTx);

    let prestate_result = Arc::try_unwrap(prestate_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();

    assert!(prestate_result.1.contains_key(&contract_address));
}

#[test]
fn test_prestate_tracer_diff_mode() {
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<TestedLatestVm>();
    let contract = TestContract::simple_transfer().bytecode;
    let account = &mut vm.rich_accounts[0];
    let tx = account.get_deploy_tx(contract, None, TxType::L2).tx;
    let nonce = tx.nonce().unwrap().0.into();
    vm.vm.push_transaction(tx);
    vm.vm.execute(InspectExecutionMode::OneTx);
    let deployed_address = deployed_address_create(account.address, nonce);
    vm.test_contract = Some(deployed_address);

    // Deploy a second copy of the contract to see its appearance in the pre-state
    let tx2 = account.get_deploy_tx(contract, None, TxType::L2).tx;
    let nonce2 = tx2.nonce().unwrap().0.into();
    vm.vm.push_transaction(tx2);
    vm.vm.execute(InspectExecutionMode::OneTx);
    let deployed_address2 = deployed_address_create(account.address, nonce2);

    let account = &mut vm.rich_accounts[0];

    //enter ether to contract to see difference in the balance post execution
    let tx0 = Execute {
        contract_address: Some(vm.test_contract.unwrap()),
        calldata: Default::default(),
        value: U256::from(100000),
        factory_deps: vec![],
    };

    vm.vm
        .push_transaction(account.get_l2_tx_for_execute(tx0.clone(), None));

    let tx1 = Execute {
        contract_address: Some(deployed_address2),
        calldata: Default::default(),
        value: U256::from(200000),
        factory_deps: vec![],
    };

    vm.vm
        .push_transaction(account.get_l2_tx_for_execute(tx1, None));
    let prestate_tracer_result = Arc::new(OnceCell::default());
    let prestate_tracer = PrestateTracer::new(true, prestate_tracer_result.clone());
    let tracer_ptr = prestate_tracer.into_tracer_pointer();
    vm.vm
        .inspect(&mut tracer_ptr.into(), InspectExecutionMode::Bootloader);

    let prestate_result = Arc::try_unwrap(prestate_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();

    //assert that the pre-state contains both deployed contracts with balance zero
    assert!(prestate_result.0.contains_key(&deployed_address));
    assert!(prestate_result.0.contains_key(&deployed_address2));
    assert_eq!(
        prestate_result.0[&deployed_address].balance,
        Some(U256::zero())
    );
    assert_eq!(
        prestate_result.0[&deployed_address2].balance,
        Some(U256::zero())
    );

    //assert that the post-state contains both deployed contracts with the correct balance
    assert!(prestate_result.1.contains_key(&deployed_address));
    assert!(prestate_result.1.contains_key(&deployed_address2));
    assert_eq!(
        prestate_result.1[&deployed_address].balance,
        Some(U256::from(100000))
    );
    assert_eq!(
        prestate_result.1[&deployed_address2].balance,
        Some(U256::from(200000))
    );
}

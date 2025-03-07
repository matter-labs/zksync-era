use zksync_test_contracts::{DeployContractsTx, TestContract, TxType};
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key, h256_to_u256,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    utils::{deployed_address_create, storage_key_for_eth_balance},
    Address, Execute, U256,
};

use super::{
    default_pubdata_builder, extract_deploy_events, tester::VmTesterBuilder, ContractToDeploy,
    TestedVm,
};
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    vm_latest::utils::fee::get_batch_base_fee,
};

pub(crate) fn test_default_aa_interaction<VM: TestedVm>() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let counter = TestContract::counter().bytecode;
    let account = &mut vm.rich_accounts[0];
    let DeployContractsTx {
        tx,
        bytecode_hash,
        address,
    } = account.get_deploy_tx(counter, None, TxType::L2);
    let maximal_fee = tx.gas_limit() * get_batch_base_fee(&vm.l1_batch_env);

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    let batch_result = vm.vm.finish_batch(default_pubdata_builder());
    assert!(
        !batch_result.block_tip_execution_result.result.is_failed(),
        "Batch tip execution wasn't successful: {:#?}",
        batch_result.block_tip_execution_result
    );

    vm.vm.get_current_execution_state();

    // Both deployment and ordinary nonce should be incremented by one.
    let account_nonce_key = get_nonce_key(&account.address);
    let expected_nonce = TX_NONCE_INCREMENT + DEPLOYMENT_NONCE_INCREMENT;

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&address);

    let operator_balance_key = storage_key_for_eth_balance(&vm.l1_batch_env.fee_account);
    let expected_fee = maximal_fee
        - U256::from(result.refunds.gas_refunded)
            * U256::from(get_batch_base_fee(&vm.l1_batch_env));

    let expected_slots = [
        (account_nonce_key, expected_nonce),
        (known_codes_key, 1.into()),
        (account_code_key, h256_to_u256(bytecode_hash)),
        (operator_balance_key, expected_fee),
    ];
    vm.vm.verify_required_storage(&expected_slots);
}

pub(crate) fn test_permissive_aa_works<VM: TestedVm>() {
    let builder = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(2);
    let aa_address = builder.rich_account(0).address();
    let mut vm = builder
        .with_custom_contracts(vec![ContractToDeploy::account(
            TestContract::permissive_account().bytecode.to_vec(),
            aa_address,
        )])
        .build::<VM>();
    let other_account = &mut vm.rich_accounts[1];

    // Check that the account can be called as a contract.
    let execute = Execute {
        contract_address: Some(aa_address),
        calldata: TestContract::permissive_account()
            .function("deploy")
            .encode_input(&[ethabi::Token::Uint(5.into())])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![TestContract::permissive_account().dependencies[0]
            .bytecode
            .to_vec()],
    };
    let tx = other_account.get_l2_tx_for_execute(execute, None);
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{res:#?}");

    let deploy_events = extract_deploy_events(&res.logs.events);
    let expected_deploy_events: Vec<_> = (0_u32..5)
        .map(|nonce| {
            (
                aa_address,
                deployed_address_create(aa_address, nonce.into()),
            )
        })
        .collect();
    assert_eq!(deploy_events, expected_deploy_events);

    let account = &mut vm.rich_accounts[0];
    let execute = Execute {
        contract_address: Some(Address::repeat_byte(1)),
        calldata: vec![],
        value: 123.into(),
        factory_deps: vec![],
    };
    let transfer_tx = account.get_l2_tx_for_execute(execute, None);
    vm.vm.push_transaction(transfer_tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{res:#?}");

    // Check that the account works as a deployer as well.
    let deploy_tx = account
        .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
        .tx;
    vm.vm.push_transaction(deploy_tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{res:#?}");

    let deploy_events = extract_deploy_events(&res.logs.events);
    assert_eq!(
        deploy_events,
        [(aa_address, deployed_address_create(aa_address, 5.into()))]
    );
}

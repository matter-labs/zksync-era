use ethabi::Token;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{Address, Execute, U256};

use super::{default_pubdata_builder, tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt};

pub(crate) fn test_predetermined_refunded_gas<VM: TestedVm>() {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    let l1_batch = vm.l1_batch_env.clone();

    let account = &mut vm.rich_accounts[0];

    let tx = account
        .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
        .tx;
    vm.vm.push_transaction(tx.clone());
    let result = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(!result.result.is_failed());

    // If the refund provided by the operator or the final refund are the 0
    // there is no impact of the operator's refund at all and so this test does not
    // make much sense.
    assert!(
        result.refunds.operator_suggested_refund > 0,
        "The operator's refund is 0"
    );
    assert!(result.refunds.gas_refunded > 0, "The final refund is 0");

    let result_without_predefined_refunds = vm
        .vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    let mut current_state_without_predefined_refunds = vm.vm.get_current_execution_state();
    assert!(!result_without_predefined_refunds.result.is_failed(),);

    // Here we want to provide the same refund from the operator and check that it's the correct one.
    // We execute the whole block without refund tracer, because refund tracer will eventually override the provided refund.
    // But the overall result should be the same

    let mut vm = VmTesterBuilder::new()
        .with_l1_batch_env(l1_batch.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    assert_eq!(account.address(), vm.rich_accounts[0].address());

    vm.vm
        .push_transaction_with_refund(tx.clone(), result.refunds.gas_refunded);

    let result_with_predefined_refunds = vm
        .vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    let mut current_state_with_predefined_refunds = vm.vm.get_current_execution_state();

    assert!(!result_with_predefined_refunds.result.is_failed());

    // We need to sort these lists as those are flattened from HashMaps
    current_state_with_predefined_refunds
        .used_contract_hashes
        .sort();
    current_state_without_predefined_refunds
        .used_contract_hashes
        .sort();

    assert_eq!(
        current_state_with_predefined_refunds.events,
        current_state_without_predefined_refunds.events
    );

    assert_eq!(
        current_state_with_predefined_refunds.user_l2_to_l1_logs,
        current_state_without_predefined_refunds.user_l2_to_l1_logs
    );

    assert_eq!(
        current_state_with_predefined_refunds.system_logs,
        current_state_without_predefined_refunds.system_logs
    );

    assert_eq!(
        current_state_with_predefined_refunds.deduplicated_storage_logs,
        current_state_without_predefined_refunds.deduplicated_storage_logs
    );
    assert_eq!(
        current_state_with_predefined_refunds.used_contract_hashes,
        current_state_without_predefined_refunds.used_contract_hashes
    );

    // In this test we put the different refund from the operator.
    // We still can't use the refund tracer, because it will override the refund.
    // But we can check that the logs and events have changed.
    let mut vm = VmTesterBuilder::new()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    assert_eq!(account.address(), vm.rich_accounts[0].address());

    let changed_operator_suggested_refund = result.refunds.gas_refunded + 1000;
    vm.vm
        .push_transaction_with_refund(tx, changed_operator_suggested_refund);
    let result = vm
        .vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    let mut current_state_with_changed_predefined_refunds = vm.vm.get_current_execution_state();

    assert!(!result.result.is_failed());
    current_state_with_changed_predefined_refunds
        .used_contract_hashes
        .sort();
    current_state_without_predefined_refunds
        .used_contract_hashes
        .sort();

    assert_eq!(
        current_state_with_changed_predefined_refunds.events.len(),
        current_state_without_predefined_refunds.events.len()
    );

    assert_ne!(
        current_state_with_changed_predefined_refunds.events,
        current_state_without_predefined_refunds.events
    );

    assert_eq!(
        current_state_with_changed_predefined_refunds.user_l2_to_l1_logs,
        current_state_without_predefined_refunds.user_l2_to_l1_logs
    );

    assert_ne!(
        current_state_with_changed_predefined_refunds.system_logs,
        current_state_without_predefined_refunds.system_logs
    );

    assert_eq!(
        current_state_with_changed_predefined_refunds
            .deduplicated_storage_logs
            .len(),
        current_state_without_predefined_refunds
            .deduplicated_storage_logs
            .len()
    );

    assert_ne!(
        current_state_with_changed_predefined_refunds.deduplicated_storage_logs,
        current_state_without_predefined_refunds.deduplicated_storage_logs
    );
    assert_eq!(
        current_state_with_changed_predefined_refunds.used_contract_hashes,
        current_state_without_predefined_refunds.used_contract_hashes
    );
}

pub(crate) fn test_negative_pubdata_for_transaction<VM: TestedVm>() {
    let expensive_contract_address = Address::repeat_byte(1);
    let expensive_contract = TestContract::expensive();
    let expensive_function = expensive_contract.function("expensive");
    let cleanup_function = expensive_contract.function("cleanUp");

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::expensive().bytecode.to_vec(),
            expensive_contract_address,
        )])
        .build::<VM>();

    let expensive_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(expensive_contract_address),
            calldata: expensive_function
                .encode_input(&[Token::Uint(10.into())])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(expensive_tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );

    // This transaction cleans all initial writes in the contract, thus having negative `pubdata` impact.
    let clean_up_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(expensive_contract_address),
            calldata: cleanup_function.encode_input(&[]).unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(clean_up_tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
    assert!(result.refunds.operator_suggested_refund > 0);
    assert_eq!(
        result.refunds.gas_refunded,
        result.refunds.operator_suggested_refund
    );
}

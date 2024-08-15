use ethabi::Token;
use zksync_types::{Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::{read_expensive_contract, read_test_contract},
        },
        types::internals::TransactionData,
        HistoryEnabled,
    },
};

#[test]
fn test_predetermined_refunded_gas() {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    // We need to provide the same DA validator to ensure the same logs
    let rollup_da_validator = Address::random();

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .with_rollup_pubdata_params(Some(rollup_da_validator))
        .build();
    let l1_batch = vm.vm.batch_env.clone();

    let counter = read_test_contract();
    let account = &mut vm.rich_accounts[0];

    let DeployContractsTx {
        tx,
        bytecode_hash: _,
        address: _,
    } = account.get_deploy_tx(&counter, None, TxType::L2);
    vm.vm.push_transaction(tx.clone());
    let result = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(!result.result.is_failed());

    // If the refund provided by the operator or the final refund are the 0
    // there is no impact of the operator's refund at all and so this test does not
    // make much sense.
    assert!(
        result.refunds.operator_suggested_refund > 0,
        "The operator's refund is 0"
    );
    assert!(result.refunds.gas_refunded > 0, "The final refund is 0");

    let result_without_predefined_refunds = vm.vm.execute(VmExecutionMode::Batch);
    let mut current_state_without_predefined_refunds = vm.vm.get_current_execution_state();
    assert!(!result_without_predefined_refunds.result.is_failed(),);

    // Here we want to provide the same refund from the operator and check that it's the correct one.
    // We execute the whole block without refund tracer, because refund tracer will eventually override the provided refund.
    // But the overall result should be the same

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![account.clone()])
        .with_rollup_pubdata_params(Some(rollup_da_validator))
        .build();

    let tx: TransactionData = tx.into();
    // Overhead
    let overhead = tx.overhead_gas();
    vm.vm
        .push_raw_transaction(tx.clone(), overhead, result.refunds.gas_refunded, true);

    let result_with_predefined_refunds = vm.vm.execute(VmExecutionMode::Batch);
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
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_l1_batch_env(l1_batch)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![account.clone()])
        .with_rollup_pubdata_params(Some(rollup_da_validator))
        .build();

    let changed_operator_suggested_refund = result.refunds.gas_refunded + 1000;
    vm.vm
        .push_raw_transaction(tx, overhead, changed_operator_suggested_refund, true);
    let result = vm.vm.execute(VmExecutionMode::Batch);
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

#[test]
fn negative_pubdata_for_transaction() {
    let expensive_contract_address = Address::random();
    let (expensive_contract_bytecode, expensive_contract) = read_expensive_contract();
    let expensive_function = expensive_contract.function("expensive").unwrap();
    let cleanup_function = expensive_contract.function("cleanUp").unwrap();

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![(
            expensive_contract_bytecode,
            expensive_contract_address,
            false,
        )])
        .build();

    let expensive_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: expensive_contract_address,
            calldata: expensive_function
                .encode_input(&[Token::Uint(10.into())])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(expensive_tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );

    // This transaction cleans all initial writes in the contract, thus having negative `pubdata` impact.
    let clean_up_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: expensive_contract_address,
            calldata: cleanup_function.encode_input(&[]).unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(clean_up_tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
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

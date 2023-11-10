use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterface};
use crate::vm_virtual_blocks::tests::tester::{DeployContractsTx, TxType, VmTesterBuilder};
use crate::vm_virtual_blocks::tests::utils::read_test_contract;

use crate::vm_latest::HistoryEnabled;
use crate::vm_virtual_blocks::types::internals::TransactionData;

#[test]
fn test_predetermined_refunded_gas() {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
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
        .build();

    let tx: TransactionData = tx.into();
    let block_gas_per_pubdata_byte = vm.vm.batch_env.block_gas_price_per_pubdata();
    // Overhead
    let overhead = tx.overhead_gas(block_gas_per_pubdata_byte as u32);
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
        current_state_with_predefined_refunds.l2_to_l1_logs,
        current_state_without_predefined_refunds.l2_to_l1_logs
    );

    assert_eq!(
        current_state_with_predefined_refunds.storage_log_queries,
        current_state_without_predefined_refunds.storage_log_queries
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
        current_state_with_changed_predefined_refunds.l2_to_l1_logs,
        current_state_without_predefined_refunds.l2_to_l1_logs
    );

    assert_eq!(
        current_state_with_changed_predefined_refunds
            .storage_log_queries
            .len(),
        current_state_without_predefined_refunds
            .storage_log_queries
            .len()
    );

    assert_ne!(
        current_state_with_changed_predefined_refunds.storage_log_queries,
        current_state_without_predefined_refunds.storage_log_queries
    );
    assert_eq!(
        current_state_with_changed_predefined_refunds.used_contract_hashes,
        current_state_without_predefined_refunds.used_contract_hashes
    );
}

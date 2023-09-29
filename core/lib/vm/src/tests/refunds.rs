use crate::tests::tester::{DeployContractsTx, TransactionTestInfo, TxType, VmTesterBuilder};
use crate::tests::utils::read_test_contract;
use crate::types::inputs::system_env::TxExecutionMode;
use zksync_types::{Execute, U256};

use crate::types::internals::TransactionData;
use crate::VmExecutionMode;

#[test]
fn test_predetermined_refunded_gas() {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    let mut vm = VmTesterBuilder::new()
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

    let mut vm = VmTesterBuilder::new()
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
    let mut vm = VmTesterBuilder::new()
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

/// Regression test against a bug that allowed spending of pubdata without ever paying for it
#[test]
fn pubdata_is_paid_for() {
    let code = zksync_contracts::read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/pubdata/Exploit.sol/Exploit.json",
    );

    let do_nothing_calldata = vec![0x32, 0x1f, 0x00, 0x13];
    let use_storage_calldata = vec![0x57, 0x97, 0xd9, 0xa4];

    let with_exploit = {
        let mut vm = VmTesterBuilder::new()
            .with_empty_in_memory_storage()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_random_rich_accounts(1)
            .build();

        let DeployContractsTx {
            tx: deploy_tx,
            bytecode_hash: _,
            address: _,
        } = vm.rich_accounts[0].get_deploy_tx(&code, None, TxType::L2);

        vm.execute_tx_and_verify(TransactionTestInfo::new_processed(deploy_tx, false));
        let exploit_address =
            zksync_types::utils::deployed_address_create(vm.rich_accounts[0].address, U256::zero());

        let do_nothing_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
            Execute {
                contract_address: exploit_address,
                calldata: do_nothing_calldata,
                value: U256::zero(),
                factory_deps: None,
            },
            None,
        );
        vm.execute_tx_and_verify(TransactionTestInfo::new_processed(do_nothing_tx, false));

        let use_storage_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
            Execute {
                contract_address: exploit_address,
                calldata: use_storage_calldata.clone(),
                value: U256::zero(),
                factory_deps: None,
            },
            None,
        );
        let zksync_types::ExecuteTransactionCommon::L2(ref d) = use_storage_tx.common_data else {
            unreachable!()
        };
        let gas_limit = d.fee.gas_limit.low_u32();

        gas_limit
            - vm.execute_tx_and_verify(TransactionTestInfo::new_processed(use_storage_tx, false))
                .refunds
                .operator_suggested_refund
    };

    let without_exploit = {
        let mut vm = VmTesterBuilder::new()
            .with_empty_in_memory_storage()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_random_rich_accounts(1)
            .build();

        let DeployContractsTx {
            tx: deploy_tx,
            bytecode_hash: _,
            address: _,
        } = vm.rich_accounts[0].get_deploy_tx(&code, None, TxType::L2);

        vm.execute_tx_and_verify(TransactionTestInfo::new_processed(deploy_tx, false));
        let exploit_address =
            zksync_types::utils::deployed_address_create(vm.rich_accounts[0].address, U256::zero());

        let use_storage_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
            Execute {
                contract_address: exploit_address,
                calldata: use_storage_calldata,
                value: U256::zero(),
                factory_deps: None,
            },
            None,
        );
        let zksync_types::ExecuteTransactionCommon::L2(ref d) = use_storage_tx.common_data else {
            unreachable!()
        };
        let gas_limit = d.fee.gas_limit.low_u32();

        gas_limit
            - vm.execute_tx_and_verify(TransactionTestInfo::new_processed(use_storage_tx, false))
                .refunds
                .operator_suggested_refund
    };

    assert_eq!(
        with_exploit, without_exploit,
        "Contract that didn't pay for any pubdata affected pricing of the next."
    );
    assert_eq!(with_exploit, 42917137);
}

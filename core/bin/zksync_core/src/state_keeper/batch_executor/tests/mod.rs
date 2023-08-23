use assert_matches::assert_matches;
use db_test_macro::db_test;

use zksync_dal::ConnectionPool;
use zksync_types::{tx::tx_execution_info::TxExecutionStatus, PriorityOpId};

mod tester;

use self::tester::{Account, Tester};
use super::TxExecutionResult;
use crate::state_keeper::batch_executor::tests::tester::TestConfig;

/// Ensures that the transaction was executed successfully.
fn assert_executed(execution_result: &TxExecutionResult) {
    assert_matches!(execution_result, TxExecutionResult::Success { .. });
}

/// Ensures that the transaction was rejected by the VM.
fn assert_rejected(execution_result: &TxExecutionResult) {
    assert_matches!(execution_result, TxExecutionResult::RejectedByVm { .. });
}

/// Ensures that the transaction was executed successfully but reverted by the VM.
fn assert_reverted(execution_result: &TxExecutionResult) {
    assert_executed(execution_result);
    if let TxExecutionResult::Success { tx_result, .. } = execution_result {
        assert_matches!(tx_result.status, TxExecutionStatus::Failure);
    } else {
        unreachable!();
    }
}

/// Checks that we can successfully execute a single L2 tx in batch executor.
#[db_test]
async fn execute_l2_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);
    executor.finish_batch().await;
}

/// Checks that we can successfully execute a single L1 tx in batch executor.
#[db_test]
async fn execute_l1_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1))).await;
    assert_executed(&res);
    executor.finish_batch().await;
}

/// Checks that we can successfully execute a single L2 tx and a single L1 tx in batch executor.
#[db_test]
async fn execute_l2_and_l1_txs(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);

    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1))).await;
    assert_executed(&res);

    executor.finish_batch().await;
}

/// Checks that we can successfully rollback the transaction and execute it once again.
#[db_test]
async fn rollback(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let tx = alice.execute();
    let res_old = executor.execute_tx(tx.clone()).await;
    assert_executed(&res_old);

    executor.rollback_last_tx().await;

    // Execute the same transaction, it must succeed.
    let res_new = executor.execute_tx(tx).await;
    assert_executed(&res_new);

    let (
        TxExecutionResult::Success {
            tx_metrics: tx_metrics_old,
            ..
        },
        TxExecutionResult::Success {
            tx_metrics: tx_metrics_new,
            ..
        },
    ) = (res_old, res_new)
    else {
        unreachable!();
    };

    assert_eq!(
        tx_metrics_old, tx_metrics_new,
        "Execution results must be the same"
    );
    executor.finish_batch().await;
}

/// Checks that incorrect transactions are marked as rejected.
#[db_test]
async fn reject_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    let executor = tester.create_batch_executor().await;

    // Wallet is not funded, it can't pay for fees.
    let res = executor.execute_tx(alice.execute()).await;
    assert_rejected(&res);
    executor.finish_batch().await;
}

/// Checks that tx with too big gas limit is correctly rejected.
#[db_test]
async fn too_big_gas_limit(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let bad_tx = alice.execute_with_gas_limit(u32::MAX);
    let res_old = executor.execute_tx(bad_tx.clone()).await;
    assert_rejected(&res_old);

    executor.rollback_last_tx().await;
    let res_new = executor.execute_tx(bad_tx).await;
    assert_rejected(&res_new);

    let (
        TxExecutionResult::RejectedByVm {
            rejection_reason: rejection_reason_old,
            ..
        },
        TxExecutionResult::RejectedByVm {
            rejection_reason: rejection_reason_new,
            ..
        },
    ) = (res_old, res_new)
    else {
        unreachable!();
    };
    assert_eq!(
        rejection_reason_old, rejection_reason_new,
        "Rejection reasons must be the same"
    );

    // Ensure that now we can execute a valid tx.
    alice.nonce -= 1; // Reset the nonce.
    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);
    executor.finish_batch().await;
}

/// Checks that we can't execute the same transaction twice.
#[db_test]
async fn tx_cant_be_reexecuted(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let tx = alice.execute();
    let res1 = executor.execute_tx(tx.clone()).await;
    assert_executed(&res1);

    // Nonce is used for the second tx.
    let res2 = executor.execute_tx(tx).await;
    assert_rejected(&res2);
    executor.finish_batch().await;
}

/// Checks that we can deploy and call the loadnext contract.
#[db_test]
async fn deploy_and_call_loadtest(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let (deploy_tx, loadtest_address) = alice.deploy_loadnext_tx();
    assert_executed(&executor.execute_tx(deploy_tx).await);
    let custom_gas_tx = alice.loadnext_custom_gas_call(loadtest_address, 10, 10_000_000);
    assert_executed(&executor.execute_tx(custom_gas_tx).await);
    let custom_writes_tx = alice.loadnext_custom_writes_call(loadtest_address, 1, 500_000_000);
    assert_executed(&executor.execute_tx(custom_writes_tx).await);
    executor.finish_batch().await;
}

/// Checks that a tx that is reverted by the VM still can be included into a batch.
#[db_test]
async fn execute_reverted_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let (deploy_tx, loadtest_address) = alice.deploy_loadnext_tx();
    assert_executed(&executor.execute_tx(deploy_tx).await);

    let custom_writes_tx = alice.loadnext_custom_writes_call(
        loadtest_address,
        1,
        1_000_000, // We provide enough gas for tx to be executed, but not enough for the call to be successful.
    );
    assert_reverted(&executor.execute_tx(custom_writes_tx).await);
    executor.finish_batch().await;
}

/// Runs the batch executor through a semi-realistic basic scenario:
/// a batch with different operations, both successful and not.
#[db_test]
async fn execute_realistic_scenario(connection_pool: ConnectionPool) {
    let mut alice = Account::random();
    let mut bob = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    tester.fund(&[bob.address()]).await;
    let executor = tester.create_batch_executor().await;

    // A good tx should be executed successfully.
    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);

    // Execute a good tx successfully, roll if back, and execute it again.
    let tx_to_be_rolled_back = alice.execute();
    let res = executor.execute_tx(tx_to_be_rolled_back.clone()).await;
    assert_executed(&res);

    executor.rollback_last_tx().await;

    let res = executor.execute_tx(tx_to_be_rolled_back.clone()).await;
    assert_executed(&res);

    // A good tx from a different account should be executed successfully.
    let res = executor.execute_tx(bob.execute()).await;
    assert_executed(&res);

    // If we try to execute an already executed again it should be rejected.
    let res = executor.execute_tx(tx_to_be_rolled_back).await;
    assert_rejected(&res);

    // An unrelated good tx should be executed successfully.
    executor.rollback_last_tx().await; // Roll back the vm to the pre-rejected-tx state.

    // No need to reset the nonce because a tx with the current nonce was indeed executed.
    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);

    // A good L1 tx should also be executed successfully.
    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1))).await;
    assert_executed(&res);

    executor.finish_batch().await;
}

/// Checks that we handle the bootloader out of gas error on execution phase.
#[db_test]
async fn bootloader_out_of_gas_for_any_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::with_config(
        connection_pool,
        TestConfig {
            save_call_traces: false,
            vm_gas_limit: Some(10),
            max_allowed_tx_gas_limit: u32::MAX,
            validation_computational_gas_limit: u32::MAX,
        },
    );

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let res = executor.execute_tx(alice.execute()).await;
    assert_matches!(res, TxExecutionResult::BootloaderOutOfGasForTx);

    executor.finish_batch().await;
}

/// Checks that we can handle the bootloader out of gas error on tip phase.
#[db_test]
#[ignore] // This test fails.
async fn bootloader_tip_out_of_gas(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let executor = tester.create_batch_executor().await;

    let res = executor.execute_tx(alice.execute()).await;
    assert_executed(&res);

    let vm_block_res = executor.finish_batch().await;

    // Just a bit below the gas used for the previous batch execution should be fine to execute the tx
    // but not enough to execute the block tip.
    tester.set_config(TestConfig {
        save_call_traces: false,
        vm_gas_limit: Some(vm_block_res.full_result.gas_used - 10),
        max_allowed_tx_gas_limit: u32::MAX,
        validation_computational_gas_limit: u32::MAX,
    });

    let second_executor = tester.create_batch_executor().await;

    let res = second_executor.execute_tx(alice.execute()).await;
    assert_matches!(res, TxExecutionResult::BootloaderOutOfGasForBlockTip);

    second_executor.finish_batch().await;
}

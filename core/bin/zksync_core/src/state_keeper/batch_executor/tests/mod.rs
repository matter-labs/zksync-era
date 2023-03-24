use crate::state_keeper::batch_executor::tests::tester::TestConfig;

use self::tester::{Account, Tester};
use assert_matches::assert_matches;
use db_test_macro::db_test;
use vm::TxRevertReason;
use zksync_types::{tx::tx_execution_info::TxExecutionStatus, PriorityOpId};

mod tester;

use super::TxExecutionResult;

/// Ensures that the transaction was executed successfully.
fn assert_executed(execution_result: TxExecutionResult) {
    assert_matches!(execution_result.tx_result, Ok(_));
    assert_matches!(execution_result.bootloader_dry_run_result, Some(Ok(_)));
    assert_matches!(execution_result.tx_metrics, Some(_));
    assert_matches!(execution_result.bootloader_dry_run_metrics, Some(_));
}

/// Ensures that the transaction was rejected.
fn assert_rejected(execution_result: TxExecutionResult) {
    assert_matches!(execution_result.tx_result, Err(_));
    assert_matches!(execution_result.bootloader_dry_run_result, None);
    assert_matches!(execution_result.tx_metrics, None);
    assert_matches!(execution_result.bootloader_dry_run_metrics, None);
}

/// Ensures that the transaction was executed successfully but reverted by the VM.
fn assert_reverted(execution_result: TxExecutionResult) {
    assert_executed(execution_result.clone());
    assert_matches!(
        execution_result.tx_result.unwrap().status,
        TxExecutionStatus::Failure
    );
}

/// Checks that we can successfully execute a single L2 tx in batch executor.
#[db_test]
async fn execute_l2_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let res = executor.execute_tx(alice.execute());
    assert_executed(res);
    executor.finish_batch();
}

/// Checks that we can successfully execute a single L1 tx in batch executor.
#[db_test]
async fn execute_l1_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1)));
    assert_executed(res);
    executor.finish_batch();
}

/// Checks that we can successfully execute a single L2 tx and a single L1 tx in batch executor.
#[db_test]
async fn execute_l2_and_l1_txs(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let res = executor.execute_tx(alice.execute());
    assert_executed(res);

    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1)));
    assert_executed(res);

    executor.finish_batch();
}

/// Checks that we can successfully rollback the transaction and execute it once again.
#[db_test]
async fn rollback(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let tx = alice.execute();
    let res_old = executor.execute_tx(tx.clone());
    assert_executed(res_old.clone());

    executor.rollback_last_tx();

    // Execute the same transaction, it must succeed.
    let res_new = executor.execute_tx(tx);
    assert_executed(res_new.clone());

    assert_eq!(
        res_old.tx_metrics, res_new.tx_metrics,
        "Execution results must be the same"
    );

    executor.finish_batch();
}

/// Checks that incorrect transactions are marked as rejected.
#[db_test]
async fn reject_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    let executor = tester.create_batch_executor();

    // Wallet is not funded, it can't pay for fees.
    let res = executor.execute_tx(alice.execute());
    assert_rejected(res);
    executor.finish_batch();
}

/// Checks that tx with too big gas limit is correctly rejected.
#[db_test]
async fn too_big_gas_limit(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let bad_tx = alice.execute_with_gas_limit(u32::MAX);
    let res1 = executor.execute_tx(bad_tx.clone());
    assert_rejected(res1.clone());

    executor.rollback_last_tx();

    let res2 = executor.execute_tx(bad_tx);
    assert_rejected(res2.clone());

    assert_eq!(res1, res2);

    // Ensure that now we can execute a valid tx.
    alice.nonce -= 1; // Reset the nonce.
    let res3 = executor.execute_tx(alice.execute());
    assert_executed(res3);
    executor.finish_batch();
}

/// Checks that we can't execute the same transaction twice.
#[db_test]
async fn tx_cant_be_reexecuted(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let tx = alice.execute();
    let res1 = executor.execute_tx(tx.clone());
    assert_executed(res1);

    // Nonce is used for the second tx.
    let res2 = executor.execute_tx(tx);
    assert_rejected(res2);
    executor.finish_batch();
}

/// Checks that we can deploy and call the loadnext contract.
#[db_test]
async fn deploy_and_call_loadtest(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let (deploy_tx, loadtest_address) = alice.deploy_loadnext_tx();
    assert_executed(executor.execute_tx(deploy_tx));
    assert_executed(executor.execute_tx(alice.loadnext_custom_gas_call(
        loadtest_address,
        10,
        10000000,
    )));
    assert_executed(executor.execute_tx(alice.loadnext_custom_writes_call(
        loadtest_address,
        1,
        500_000_000,
    )));
    executor.finish_batch();
}

/// Checks that a tx that is reverted by the VM still can be included into a batch.
#[db_test]
async fn execute_reverted_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let (deploy_tx, loadtest_address) = alice.deploy_loadnext_tx();
    assert_executed(executor.execute_tx(deploy_tx));

    assert_reverted(executor.execute_tx(alice.loadnext_custom_writes_call(
        loadtest_address,
        1,
        1_000_000, // We provide enough gas for tx to be executed, but not enough for the call to be successful.
    )));
    executor.finish_batch();
}

/// Runs the batch executor through a semi-realistic basic scenario:
/// a batch with different operations, both successful and not.
#[db_test]
async fn execute_realistic_scenario(connection_pool: ConnectionPool) {
    let mut alice = Account::random();
    let mut bob = Account::random();

    let tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]);
    tester.fund(&[bob.address()]);
    let executor = tester.create_batch_executor();

    // A good tx should be executed successfully.
    let res = executor.execute_tx(alice.execute());
    assert_executed(res);

    // Execute a good tx successfully, roll if back, and execute it again.
    let tx_to_be_rolled_back = alice.execute();
    let res = executor.execute_tx(tx_to_be_rolled_back.clone());
    assert_executed(res);

    executor.rollback_last_tx();

    let res = executor.execute_tx(tx_to_be_rolled_back.clone());
    assert_executed(res);

    // A good tx from a different account should be executed successfully.
    let res = executor.execute_tx(bob.execute());
    assert_executed(res);

    // If we try to execute an already executed again it should be rejected.
    let res = executor.execute_tx(tx_to_be_rolled_back);
    assert_rejected(res);

    // An unrelated good tx should be executed successfully.
    executor.rollback_last_tx(); // Roll back the vm to the pre-rejected-tx state.

    // No need to reset the nonce because a tx with the current nonce was indeed executed.
    let res = executor.execute_tx(alice.execute());
    assert_executed(res);

    // A good L1 tx should also be executed successfully.
    let res = executor.execute_tx(alice.l1_execute(PriorityOpId(1)));
    assert_executed(res);

    executor.finish_batch();
}

/// Checks that we handle the bootloader out of gas error on execution phase.
#[db_test]
async fn bootloader_out_of_gas_for_any_tx(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let tester = Tester::with_config(
        connection_pool,
        TestConfig {
            reexecute_each_tx: false,
            vm_gas_limit: Some(10),
            max_allowed_tx_gas_limit: u32::MAX,
            validation_computational_gas_limit: u32::MAX,
        },
    );

    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let res = executor.execute_tx(alice.execute());

    assert_rejected(res.clone());
    assert_matches!(res.err().unwrap(), TxRevertReason::BootloaderOutOfGas);

    executor.finish_batch();
}

/// Checks that we can handle the bootloader out of gas error on tip phase.
#[db_test]
#[ignore] // This test is blocked by [PLA-50] as gas calculation are affected by the underflow.
async fn bootloader_tip_out_of_gas(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]);
    let executor = tester.create_batch_executor();

    let res = executor.execute_tx(alice.execute());
    assert_executed(res);

    let vm_block_res = executor.finish_batch();

    // Just a bit below the gas used for the previous batch execution should be fine to execute the tx
    // but not enough to execute the block tip.
    tester.set_config(TestConfig {
        reexecute_each_tx: false,
        vm_gas_limit: Some(vm_block_res.full_result.gas_used - 10),
        max_allowed_tx_gas_limit: u32::MAX,
        validation_computational_gas_limit: u32::MAX,
    });

    let second_executor = tester.create_batch_executor();

    let res = second_executor.execute_tx(alice.execute());
    assert!(res.tx_result.is_ok());
    assert_matches!(
        res.bootloader_dry_run_result,
        Some(Err(TxRevertReason::BootloaderOutOfGas))
    );

    second_executor.finish_batch();
}

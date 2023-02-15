use crate::state_keeper::batch_executor::tests::tester::TestConfig;

use self::tester::{Account, Tester};
use assert_matches::assert_matches;
use db_test_macro::db_test;
use vm::{utils::BLOCK_GAS_LIMIT, TxRevertReason};
use zksync_types::PriorityOpId;

mod tester;

use super::TxExecutionResult;

/// Ensures that transaction was executed successfully.
fn assert_executed(execution_result: TxExecutionResult) {
    assert_matches!(execution_result.tx_result, Ok(_));
    assert_matches!(execution_result.bootloader_dry_run_result, Some(Ok(_)));
    assert_matches!(execution_result.tx_metrics, Some(_));
    assert_matches!(execution_result.bootloader_dry_run_metrics, Some(_));
}

/// Ensures that transaction was rejected.
fn assert_rejected(execution_result: TxExecutionResult) {
    assert_matches!(execution_result.tx_result, Err(_));
    assert_matches!(execution_result.bootloader_dry_run_result, None);
    assert_matches!(execution_result.tx_metrics, None);
    assert_matches!(execution_result.bootloader_dry_run_metrics, None);
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

/// Checks that we can successfully rollback the transaction and execute it once again.
#[db_test]
#[ignore]
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

/// Checks that we tx with too big gas limit is correctly rejected.
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
    assert_executed(executor.execute_tx(alice.loadnext_custom_writes_call(loadtest_address, 1)));
    executor.finish_batch();
}

/// Checks that we can cause the bootloader out of gas error on tip phase.
#[db_test]
#[ignore]
async fn bootloader_tip_out_of_gas(connection_pool: ConnectionPool) {
    let mut alice = Account::random();

    // Disable the gas limit check.
    let tester = Tester::with_config(
        connection_pool,
        TestConfig {
            reexecute_each_tx: false,
            max_allowed_tx_gas_limit: u32::MAX,
        },
    );
    tester.genesis().await;
    tester.fund(&[alice.address()]);

    let executor = tester.create_batch_executor();

    let mut gas_remaining = BLOCK_GAS_LIMIT;
    let (deploy_tx, loadnext_address) = alice.deploy_loadnext_tx();

    let deploy_res = executor.execute_tx(deploy_tx);
    assert_executed(deploy_res.clone());
    gas_remaining -= deploy_res.tx_metrics.unwrap().execution_metrics.gas_used as u32;

    let consume_gas_tx =
        alice.loadnext_custom_gas_call(loadnext_address, gas_remaining, gas_remaining);
    let res = executor.execute_tx(consume_gas_tx);

    assert!(res.tx_result.is_ok());
    assert_matches!(
        res.bootloader_dry_run_result,
        Some(Err(TxRevertReason::BootloaderOutOfGas))
    );
    executor.finish_batch();
}

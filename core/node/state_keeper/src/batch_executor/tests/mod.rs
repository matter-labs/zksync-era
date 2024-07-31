use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_contracts::l2_message_root;
use zksync_dal::{ConnectionPool, Core};
use zksync_test_account::Account;
use zksync_types::{
    get_nonce_key, utils::storage_key_for_eth_balance, Execute, PriorityOpId,
    L2_MESSAGE_ROOT_ADDRESS, U256,
};

use self::tester::{AccountLoadNextExecutable, StorageSnapshot, TestConfig, Tester};
use super::TxExecutionResult;

mod read_storage_factory;
mod tester;

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
        assert!(tx_result.result.is_failed());
    } else {
        unreachable!();
    }
}

#[derive(Debug, Clone, Copy)]
enum StorageType {
    AsyncRocksdbCache,
    Rocksdb,
    Postgres,
}

impl StorageType {
    const ALL: [Self; 3] = [Self::AsyncRocksdbCache, Self::Rocksdb, Self::Postgres];
}

/// Checks that we can successfully execute a single L2 tx in batch executor on all storage types.
#[test_casing(3, StorageType::ALL)]
#[tokio::test]
async fn execute_l2_tx(storage_type: StorageType) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            storage_type,
            &[message_root_init_txn.clone()],
        )
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);
    executor.finish_batch().await.unwrap();
}

#[derive(Debug, Clone, Copy)]
enum SnapshotRecoveryMutation {
    RemoveNonce,
    RemoveBalance,
}

impl SnapshotRecoveryMutation {
    const ALL: [Option<Self>; 3] = [None, Some(Self::RemoveNonce), Some(Self::RemoveBalance)];

    fn mutate_snapshot(self, storage_snapshot: &mut StorageSnapshot, alice: &Account) {
        match self {
            Self::RemoveNonce => {
                let nonce_key = get_nonce_key(&alice.address()).hashed_key();
                let nonce_value = storage_snapshot.storage_logs.remove(&nonce_key);
                assert!(nonce_value.is_some());
            }
            Self::RemoveBalance => {
                let balance_key = storage_key_for_eth_balance(&alice.address()).hashed_key();
                let balance_value = storage_snapshot.storage_logs.remove(&balance_key);
                assert!(balance_value.is_some());
            }
        }
    }
}

const EXECUTE_L2_TX_AFTER_SNAPSHOT_RECOVERY_CASES: Product<(
    [Option<SnapshotRecoveryMutation>; 3],
    [StorageType; 3],
)> = Product((SnapshotRecoveryMutation::ALL, StorageType::ALL));

/// Tests that we can continue executing account transactions after emulating snapshot recovery.
/// Test cases with a set `mutation` ensure that the VM executor correctly detects missing data (e.g., dropped account nonce).
#[test_casing(9, EXECUTE_L2_TX_AFTER_SNAPSHOT_RECOVERY_CASES)]
#[tokio::test]
async fn execute_l2_tx_after_snapshot_recovery(
    mutation: Option<SnapshotRecoveryMutation>,
    storage_type: StorageType,
) {
    let mut alice = Account::random();
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut storage_snapshot =
        StorageSnapshot::new(&connection_pool, &mut alice, 10, &[message_root_init_txn]).await;
    assert!(storage_snapshot.storage_logs.len() > 10); // sanity check
    assert!(!storage_snapshot.factory_deps.is_empty());
    if let Some(mutation) = mutation {
        mutation.mutate_snapshot(&mut storage_snapshot, &alice);
    }
    let snapshot = storage_snapshot.recover(&connection_pool).await;

    let mut tester = Tester::new(connection_pool);
    let mut executor = tester
        .recover_batch_executor_custom(&storage_type, &snapshot)
        .await;
    let res = executor.execute_tx(alice.execute()).await.unwrap();
    if mutation.is_none() {
        assert_executed(&res);
        executor.finish_batch().await.unwrap();
    } else {
        assert_rejected(&res);
    }
}

/// Checks that we can successfully execute a single L1 tx in batch executor.
#[tokio::test]
async fn execute_l1_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let res = executor
        .execute_tx(alice.l1_execute(PriorityOpId(1)))
        .await
        .unwrap();
    assert_executed(&res);
    executor.finish_batch().await.unwrap();
}

/// Checks that we can successfully execute a single L2 tx and a single L1 tx in batch executor.
#[tokio::test]
async fn execute_l2_and_l1_txs() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);

    let res = executor
        .execute_tx(alice.l1_execute(PriorityOpId(1)))
        .await
        .unwrap();
    assert_executed(&res);

    executor.finish_batch().await.unwrap();
}

/// Checks that we can successfully rollback the transaction and execute it once again.
#[tokio::test]
async fn rollback() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let tx = alice.execute();
    let res_old = executor.execute_tx(tx.clone()).await.unwrap();
    assert_executed(&res_old);

    executor.rollback_last_tx().await.unwrap();

    // Execute the same transaction, it must succeed.
    let res_new = executor.execute_tx(tx).await.unwrap();
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

    executor.finish_batch().await.unwrap();
}

/// Checks that incorrect transactions are marked as rejected.
#[tokio::test]
async fn reject_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    // Wallet is not funded, it can't pay for fees.
    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_rejected(&res);
}

/// Checks that tx with too big gas limit is correctly processed.
/// When processed in the bootloader, no more than 80M gas can be used within the execution context.
#[tokio::test]
async fn too_big_gas_limit() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let big_gas_limit_tx = alice.execute_with_gas_limit(u32::MAX);

    let res = executor.execute_tx(big_gas_limit_tx).await.unwrap();
    assert_executed(&res);
    executor.finish_batch().await.unwrap();
}

/// Checks that we can't execute the same transaction twice.
#[tokio::test]
async fn tx_cant_be_reexecuted() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let tx = alice.execute();
    let res1 = executor.execute_tx(tx.clone()).await.unwrap();
    assert_executed(&res1);

    // Nonce is used for the second tx.
    let res2 = executor.execute_tx(tx).await.unwrap();
    assert_rejected(&res2);
}

/// Checks that we can deploy and call the loadnext contract.
#[tokio::test]
async fn deploy_and_call_loadtest() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let tx = alice.deploy_loadnext_tx();
    assert_executed(&executor.execute_tx(tx.tx).await.unwrap());
    assert_executed(
        &executor
            .execute_tx(alice.loadnext_custom_gas_call(tx.address, 10, 10_000_000))
            .await
            .unwrap(),
    );
    assert_executed(
        &executor
            .execute_tx(alice.loadnext_custom_writes_call(tx.address, 1, 500_000_000))
            .await
            .unwrap(),
    );
    executor.finish_batch().await.unwrap();
}

/// Checks that a tx that is reverted by the VM still can be included into a batch.
#[tokio::test]
async fn execute_reverted_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address(), bob.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = bob.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    let tx = alice.deploy_loadnext_tx();
    assert_executed(&executor.execute_tx(tx.tx).await.unwrap());

    let txn = &executor
        .execute_tx(alice.loadnext_custom_writes_call(
            tx.address, 1,
            1_000_000, // We provide enough gas for tx to be executed, but not enough for the call to be successful.
        ))
        .await
        .unwrap();

    dbg!(&txn);

    assert_reverted(txn);
    executor.finish_batch().await.unwrap();
}

/// Runs the batch executor through a semi-realistic basic scenario:
/// a batch with different operations, both successful and not.
#[tokio::test]
async fn execute_realistic_scenario() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    tester.fund(&[bob.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;

    // A good tx should be executed successfully.
    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);

    // Execute a good tx successfully, roll if back, and execute it again.
    let tx_to_be_rolled_back = alice.execute();
    let res = executor
        .execute_tx(tx_to_be_rolled_back.clone())
        .await
        .unwrap();
    assert_executed(&res);

    executor.rollback_last_tx().await.unwrap();

    let res = executor
        .execute_tx(tx_to_be_rolled_back.clone())
        .await
        .unwrap();
    assert_executed(&res);

    // A good tx from a different account should be executed successfully.
    let res = executor.execute_tx(bob.execute()).await.unwrap();
    assert_executed(&res);

    // If we try to execute an already executed again it should be rejected.
    let res = executor.execute_tx(tx_to_be_rolled_back).await.unwrap();
    assert_rejected(&res);

    // An unrelated good tx should be executed successfully.
    executor.rollback_last_tx().await.unwrap(); // Roll back the vm to the pre-rejected-tx state.

    // No need to reset the nonce because a tx with the current nonce was indeed executed.
    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);

    // A good L1 tx should also be executed successfully.
    let res = executor
        .execute_tx(alice.l1_execute(PriorityOpId(1)))
        .await
        .unwrap();
    assert_executed(&res);

    executor.finish_batch().await.unwrap();
}

/// Checks that we handle the bootloader out of gas error on execution phase.
#[tokio::test]
async fn bootloader_out_of_gas_for_any_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::with_config(
        connection_pool,
        TestConfig {
            save_call_traces: false,
            vm_gas_limit: Some(10),
            validation_computational_gas_limit: u32::MAX,
        },
    );

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_matches!(res, TxExecutionResult::BootloaderOutOfGasForTx);
}

/// Checks that we can handle the bootloader out of gas error on tip phase.
#[tokio::test]
#[ignore] // This test fails.
async fn bootloader_tip_out_of_gas() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);

    let finished_batch = executor.finish_batch().await.unwrap();

    // Just a bit below the gas used for the previous batch execution should be fine to execute the tx
    // but not enough to execute the block tip.
    tester.set_config(TestConfig {
        save_call_traces: false,
        vm_gas_limit: Some(
            finished_batch
                .block_tip_execution_result
                .statistics
                .computational_gas_used
                - 10,
        ),
        validation_computational_gas_limit: u32::MAX,
    });

    let mut second_executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = second_executor.execute_tx(alice.execute()).await.unwrap();
    assert_matches!(res, TxExecutionResult::BootloaderOutOfGasForTx);
}

#[tokio::test]
async fn catchup_rocksdb_cache() {
    let connection_pool = ConnectionPool::constrained_test_pool(2).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool);

    tester.genesis().await;
    tester.fund(&[alice.address(), bob.address()]).await;

    let l2_message_root = l2_message_root();
    let encoded_data = l2_message_root
        .function("initialize")
        .unwrap()
        .encode_input(&[])
        .unwrap();

    let message_root_init_txn = alice.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_MESSAGE_ROOT_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    // Execute a bunch of transactions to populate Postgres-based storage (note that RocksDB stays empty)
    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::Postgres,
            &[message_root_init_txn.clone()],
        )
        .await;
    for _ in 0..10 {
        let res = executor.execute_tx(alice.execute()).await.unwrap();
        assert_executed(&res);
    }

    // Execute one more tx on PG
    let tx = alice.execute();
    let res = executor.execute_tx(tx.clone()).await.unwrap();
    assert_executed(&res);
    executor.finish_batch().await.unwrap();

    // Async RocksDB cache should be aware of the tx and should reject it
    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::AsyncRocksdbCache,
            &[message_root_init_txn.clone()],
        )
        .await;
    let res = executor.execute_tx(tx.clone()).await.unwrap();
    assert_rejected(&res);
    // Execute one tx just so we can finish the batch
    executor.rollback_last_tx().await.unwrap(); // Roll back the vm to the pre-rejected-tx state.
    let res = executor.execute_tx(bob.execute()).await.unwrap();
    assert_executed(&res);
    executor.finish_batch().await.unwrap();
    // Wait for all background tasks to exit, otherwise we might still be holding a RocksDB lock
    tester.wait_for_tasks().await;

    // Sync RocksDB storage should be aware of the tx and should reject it
    let mut executor = tester
        .create_batch_executor_with_init_transactions(
            StorageType::Rocksdb,
            &[message_root_init_txn.clone()],
        )
        .await;
    let res = executor.execute_tx(tx).await.unwrap();
    assert_rejected(&res);
}

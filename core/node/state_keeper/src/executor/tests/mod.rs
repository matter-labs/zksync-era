use assert_matches::assert_matches;
use rand::{thread_rng, Rng};
use test_casing::{test_casing, Product};
use zksync_contracts::l2_message_root;
use zksync_dal::{ConnectionPool, Core};
use zksync_multivm::interface::{
    BatchTransactionExecutionResult, Call, CallType, ExecutionResult, Halt, VmEvent,
};
use zksync_system_constants::{COMPRESSOR_ADDRESS, L1_MESSENGER_ADDRESS};
use zksync_test_contracts::{Account, TestContract};
use zksync_types::{
    address_to_h256, get_nonce_key,
    utils::{deployed_address_create, storage_key_for_eth_balance},
    vm::FastVmMode,
    web3, Execute, PriorityOpId, H256, L2_MESSAGE_ROOT_ADDRESS, U256,
};
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};

use self::tester::{AccountExt, StorageSnapshot, TestConfig, Tester, TRANSFER_VALUE};

mod read_storage_factory;
mod tester;

/// Ensures that the transaction was executed successfully.
fn assert_executed(execution_result: &BatchTransactionExecutionResult) {
    let result = &execution_result.tx_result.result;
    assert_matches!(
        result,
        ExecutionResult::Success { .. } | ExecutionResult::Revert { .. }
    );
}

fn assert_succeeded(execution_result: &BatchTransactionExecutionResult) {
    let result = &execution_result.tx_result.result;
    assert_matches!(result, ExecutionResult::Success { .. })
}

/// Ensures that the transaction was rejected by the VM.
fn assert_rejected(execution_result: &BatchTransactionExecutionResult) {
    let result = &execution_result.tx_result.result;
    assert_matches!(result, ExecutionResult::Halt { reason } if !matches!(reason, Halt::BootloaderOutOfGas));
}

/// Ensures that the transaction was executed successfully but reverted by the VM.
fn assert_reverted(execution_result: &BatchTransactionExecutionResult) {
    let result = &execution_result.tx_result.result;
    assert_matches!(result, ExecutionResult::Revert { .. });
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

const FAST_VM_MODES: [FastVmMode; 1] = [FastVmMode::Old];

/// Checks that we can successfully execute a single L2 tx in batch executor on all storage types.
#[test_casing(3, Product((StorageType::ALL, FAST_VM_MODES)))]
#[tokio::test]
async fn execute_l2_tx(storage_type: StorageType, vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut tester = Tester::new(connection_pool, vm_mode);
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

/// Tests that we can continue executing account transactions after emulating snapshot recovery.
/// Test cases with a set `mutation` ensure that the VM executor correctly detects missing data (e.g., dropped account nonce).
#[test_casing(9, Product((SnapshotRecoveryMutation::ALL, StorageType::ALL)))]
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);
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
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_l1_tx(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);

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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_l2_and_l1_txs(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

#[tokio::test]
async fn working_with_transient_storage() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let deploy_tx = alice.deploy_storage_tester();
    let res = executor.execute_tx(deploy_tx.tx).await.unwrap();
    assert_succeeded(&res);

    let storage_test_address = deploy_tx.address;
    let test_tx = alice.test_transient_store(storage_test_address);
    let res = executor.execute_tx(test_tx).await.unwrap();
    assert_succeeded(&res);

    let test_tx = alice.assert_transient_value(storage_test_address, 0.into());
    let res = executor.execute_tx(test_tx).await.unwrap();
    assert_succeeded(&res);

    executor.finish_batch().await.unwrap();
}

#[tokio::test]
async fn decommitting_contract() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let deploy_tx = alice.deploy_precompiles_test();
    let res = executor.execute_tx(deploy_tx.tx).await.unwrap();
    assert_succeeded(&res);

    let keccak_bytecode_hash = web3::keccak256(TestContract::precompiles_test().bytecode);
    let test_tx = alice.test_decommit(
        deploy_tx.address,
        deploy_tx.bytecode_hash,
        H256(keccak_bytecode_hash),
    );
    let res = executor.execute_tx(test_tx).await.unwrap();
    assert_succeeded(&res);

    executor.finish_batch().await.unwrap();
}

/// Checks that we can successfully rollback the transaction and execute it once again.
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn rollback(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);

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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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
    let res_new = executor.execute_tx(tx.clone()).await.unwrap();
    assert_executed(&res_new);

    let tx_metrics_old = res_old.tx_result.get_execution_metrics();
    let tx_metrics_new = res_new.tx_result.get_execution_metrics();
    assert_eq!(
        tx_metrics_old, tx_metrics_new,
        "Execution results must be the same"
    );

    executor.finish_batch().await.unwrap();
}

/// Checks that incorrect transactions are marked as rejected.
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn reject_tx(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);

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
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn too_big_gas_limit(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

#[tokio::test]
async fn check_deployment_allow_list() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

    // Alice is allowed to deploy contracts.
    let filter = DeploymentTxFilter::new(SharedAllowList::from(vec![alice.address()]));

    // Check that Alice can deploy contracts.
    let tx = alice.deploy_loadnext_tx();
    let res = executor.execute_tx(tx.tx).await.unwrap();
    assert_executed(&res);
    assert!(filter
        .find_not_allowed_deployer(alice.address(), &res.tx_result.logs.events)
        .await
        .is_none());

    // Check that Bob can't deploy contracts.
    let tx = bob.deploy_loadnext_tx();
    let res = executor.execute_tx(tx.tx).await.unwrap();
    assert_executed(&res);
    assert_eq!(
        filter
            .find_not_allowed_deployer(bob.address(), &res.tx_result.logs.events)
            .await,
        Some(bob.address())
    );

    // Check that Bob can execute non deploy transactions.
    let tx = bob.execute();
    let res = executor.execute_tx(tx).await.unwrap();
    assert_executed(&res);
    assert!(filter
        .find_not_allowed_deployer(bob.address(), &res.tx_result.logs.events)
        .await
        .is_none());
}

/// Checks that we can't execute the same transaction twice.
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn tx_cant_be_reexecuted(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);
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
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn deploy_and_call_loadtest(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);
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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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
            .execute_tx(alice.loadnext_custom_initial_writes_call(tx.address, 1, 500_000_000))
            .await
            .unwrap(),
    );
    executor.finish_batch().await.unwrap();
}

#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn deploy_failedcall(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);
    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let tx = alice.deploy_failed_call_tx();

    let execute_tx = executor.execute_tx(tx.tx).await.unwrap();
    assert_executed(&execute_tx);

    executor.finish_batch().await.unwrap();
}

/// Checks that a tx that is reverted by the VM still can be included into a batch.
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_reverted_tx(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);

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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

    assert_reverted(
        &executor
            .execute_tx(alice.loadnext_custom_initial_writes_call(
                tx.address, 1,
                1_000_000, // We provide enough gas for tx to be executed, but not enough for the call to be successful.
            ))
            .await
            .unwrap(),
    );
    executor.finish_batch().await.unwrap();
}

/// Runs the batch executor through a semi-realistic basic scenario:
/// a batch with different operations, both successful and not.
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_realistic_scenario(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool, vm_mode);

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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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
#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn bootloader_out_of_gas_for_any_tx(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::with_config(
        connection_pool,
        TestConfig {
            trace_calls: false,
            vm_gas_limit: Some(10),
            validation_computational_gas_limit: u32::MAX,
            fast_vm_mode: vm_mode,
        },
    );

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_matches!(
        res.tx_result.result,
        ExecutionResult::Halt {
            reason: Halt::BootloaderOutOfGas
        }
    );
}

/// Checks that we can handle the bootloader out of gas error on tip phase.
#[tokio::test]
#[ignore] // This test fails.
async fn bootloader_tip_out_of_gas() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = executor.execute_tx(alice.execute()).await.unwrap();
    assert_executed(&res);

    let (finished_batch, _) = executor.finish_batch().await.unwrap();

    // Just a bit below the gas used for the previous batch execution should be fine to execute the tx
    // but not enough to execute the block tip.
    tester.set_config(TestConfig {
        trace_calls: false,
        vm_gas_limit: Some(
            finished_batch
                .block_tip_execution_result
                .statistics
                .computational_gas_used
                - 10,
        ),
        validation_computational_gas_limit: u32::MAX,
        fast_vm_mode: FastVmMode::Old,
    });

    let mut second_executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let res = second_executor.execute_tx(alice.execute()).await.unwrap();
    assert_matches!(
        res.tx_result.result,
        ExecutionResult::Halt {
            reason: Halt::BootloaderOutOfGas
        }
    );
}

#[tokio::test]
async fn catchup_rocksdb_cache() {
    let connection_pool = ConnectionPool::constrained_test_pool(2).await;
    let mut alice = Account::random();
    let mut bob = Account::random();

    let mut tester = Tester::new(connection_pool, FastVmMode::Old);

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
            contract_address: Some(L2_MESSAGE_ROOT_ADDRESS),
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

#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_tx_with_large_packable_bytecode(vm_mode: FastVmMode) {
    // The rough length of the packed bytecode should be 350_000 / 4 = 87500,
    // which should fit into a batch
    const BYTECODE_LEN: usize = 350_016 + 32; // +32 to ensure validity of the bytecode

    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut tester = Tester::new(connection_pool, vm_mode);
    let mut rng = thread_rng();

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;

    let mut packable_bytecode = vec![];
    while packable_bytecode.len() < BYTECODE_LEN {
        packable_bytecode.extend_from_slice(&if rng.gen() { [0_u8; 8] } else { [0xff_u8; 8] });
    }
    let tx = alice.execute_with_factory_deps(vec![packable_bytecode.clone()]);

    let res = executor.execute_tx(tx).await.unwrap();
    assert_matches!(res.tx_result.result, ExecutionResult::Success { .. });
    res.compression_result.unwrap();

    let events = &res.tx_result.logs.events;
    // Extract compressed bytecodes from the long L2-to-L1 messages by the compressor contract.
    let compressed_bytecodes: Vec<_> = events
        .iter()
        .filter(|event| {
            event.address == L1_MESSENGER_ADDRESS
                && event.indexed_topics[0] == VmEvent::L1_MESSAGE_EVENT_SIGNATURE
                && event.indexed_topics[1] == address_to_h256(&COMPRESSOR_ADDRESS)
        })
        .map(|event| &event.value)
        .collect();

    assert_eq!(compressed_bytecodes.len(), 1);
    assert!(compressed_bytecodes[0].len() < BYTECODE_LEN / 2);

    executor.finish_batch().await.unwrap();
}

#[test_casing(1, FAST_VM_MODES)]
#[tokio::test]
async fn execute_txs_with_call_traces(vm_mode: FastVmMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut alice = Account::random();
    let mut tester = Tester::with_config(
        connection_pool,
        TestConfig {
            trace_calls: true,
            ..TestConfig::new(vm_mode)
        },
    );

    tester.genesis().await;
    tester.fund(&[alice.address()]).await;
    let mut executor = tester
        .create_batch_executor(StorageType::AsyncRocksdbCache)
        .await;
    let res = executor.execute_tx(alice.execute()).await.unwrap();

    assert_matches!(res.tx_result.result, ExecutionResult::Success { .. });
    assert!(!res.call_traces.is_empty());

    find_first_call(&res.call_traces, &|call| {
        call.from == alice.address && call.value == TRANSFER_VALUE.into()
    })
    .expect("no transfer call");

    let deploy_tx = alice.deploy_loadnext_tx().tx;
    let res = executor.execute_tx(deploy_tx).await.unwrap();
    assert_matches!(res.tx_result.result, ExecutionResult::Success { .. });
    assert!(!res.call_traces.is_empty());

    let create_call = find_first_call(&res.call_traces, &|call| {
        call.from == alice.address && call.r#type == CallType::Create
    })
    .expect("no create call");

    let expected_address = deployed_address_create(alice.address, 0.into());
    assert_eq!(create_call.to, expected_address);
    assert!(!create_call.input.is_empty());
}

fn find_first_call<'a>(calls: &'a [Call], predicate: &impl Fn(&Call) -> bool) -> Option<&'a Call> {
    for call in calls {
        if predicate(call) {
            return Some(call);
        }
        if let Some(call) = find_first_call(&call.calls, predicate) {
            return Some(call);
        }
    }
    None
}

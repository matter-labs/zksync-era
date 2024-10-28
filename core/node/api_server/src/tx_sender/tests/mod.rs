//! Tests for the transaction sender.

use test_casing::TestCases;
use zksync_contracts::test_contracts::LoadnextContractExecutionParams;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, prepare_recovery_snapshot};
use zksync_types::{get_nonce_key, L1BatchNumber, L2BlockNumber, StorageLog};
use zksync_vm_executor::oneshot::MockOneshotExecutor;

use super::*;
use crate::web3::testonly::create_test_tx_sender;

mod call;
mod gas_estimation;
mod send_tx;

const LOAD_TEST_CASES: TestCases<LoadnextContractExecutionParams> = test_casing::cases! {[
    LoadnextContractExecutionParams::default(),
    // No storage modification
    LoadnextContractExecutionParams {
        writes: 0,
        events: 0,
        ..LoadnextContractExecutionParams::default()
    },
    // Moderately deep recursion (very deep recursion is tested separately)
    LoadnextContractExecutionParams {
        recursive_calls: 10,
        ..LoadnextContractExecutionParams::default()
    },
    // No deploys
    LoadnextContractExecutionParams {
        deploys: 0,
        ..LoadnextContractExecutionParams::default()
    },
    // Lots of deploys
    LoadnextContractExecutionParams {
        deploys: 10,
        ..LoadnextContractExecutionParams::default()
    },
]};

#[tokio::test]
async fn getting_nonce_for_account() {
    let l2_chain_id = L2ChainId::default();
    let test_address = Address::repeat_byte(1);
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Manually insert a nonce for the address.
    let nonce_key = get_nonce_key(&test_address);
    let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(123));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[nonce_log])
        .await
        .unwrap();

    let tx_executor = MockOneshotExecutor::default();
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(123));

    // Insert another L2 block with a new nonce log.
    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(1))
        .await
        .unwrap();
    let nonce_log = StorageLog {
        value: H256::from_low_u64_be(321),
        ..nonce_log
    };
    storage
        .storage_logs_dal()
        .insert_storage_logs(L2BlockNumber(1), &[nonce_log])
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

#[tokio::test]
async fn getting_nonce_for_account_after_snapshot_recovery() {
    const SNAPSHOT_L2_BLOCK_NUMBER: L2BlockNumber = L2BlockNumber(42);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let test_address = Address::repeat_byte(1);
    let other_address = Address::repeat_byte(2);
    let nonce_logs = [
        StorageLog::new_write_log(get_nonce_key(&test_address), H256::from_low_u64_be(123)),
        StorageLog::new_write_log(get_nonce_key(&other_address), H256::from_low_u64_be(25)),
    ];
    prepare_recovery_snapshot(
        &mut storage,
        L1BatchNumber(23),
        SNAPSHOT_L2_BLOCK_NUMBER,
        &nonce_logs,
    )
    .await;

    let l2_chain_id = L2ChainId::default();
    let tx_executor = MockOneshotExecutor::default();
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(SNAPSHOT_L2_BLOCK_NUMBER.0 + 1))
        .await
        .unwrap();
    let new_nonce_logs = vec![StorageLog::new_write_log(
        get_nonce_key(&test_address),
        H256::from_low_u64_be(321),
    )];
    storage
        .storage_logs_dal()
        .insert_storage_logs(SNAPSHOT_L2_BLOCK_NUMBER + 1, &new_nonce_logs)
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let nonce = tx_sender.get_expected_nonce(other_address).await.unwrap();
    assert_eq!(nonce, Nonce(25));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

async fn create_real_tx_sender(pool: ConnectionPool<Core>) -> TxSender {
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    drop(storage);

    let genesis_config = genesis_params.config();
    let mut executor_options = SandboxExecutorOptions::new(
        genesis_config.l2_chain_id,
        AccountTreeId::new(genesis_config.fee_account),
        u32::MAX,
    )
    .await
    .unwrap();
    executor_options.set_fast_vm_mode(FastVmMode::Shadow);

    let pg_caches = PostgresStorageCaches::new(1, 1);
    let tx_executor = SandboxExecutor::real(executor_options, pg_caches, usize::MAX);
    create_test_tx_sender(pool, genesis_params.config().l2_chain_id, tx_executor)
        .await
        .0
}

async fn pending_block_args(tx_sender: &TxSender) -> BlockArgs {
    let mut storage = tx_sender.acquire_replica_connection().await.unwrap();
    BlockArgs::pending(&mut storage).await.unwrap()
}

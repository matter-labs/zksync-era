//! Tests for the metadata calculator component life cycle.

use std::{future::Future, ops, panic, path::Path, sync::Arc, time::Duration};

use assert_matches::assert_matches;
use itertools::Itertools;
use tempfile::TempDir;
use test_casing::{test_casing, Product};
use tokio::sync::{mpsc, watch};
use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_health_check::{CheckHealth, HealthStatus};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_object_store::{MockObjectStore, ObjectStore};
use zksync_prover_interface::inputs::PrepareBasicCircuitsJob;
use zksync_storage::RocksDB;
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, StorageKey, StorageLog, H256,
};
use zksync_utils::u32_to_h256;

use super::{
    helpers::L1BatchWithLogs, GenericAsyncTree, MetadataCalculator, MetadataCalculatorConfig,
    MetadataCalculatorRecoveryConfig,
};
use crate::helpers::{AsyncTree, Delayer};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

async fn run_with_timeout<T, F>(timeout: Duration, action: F) -> T
where
    F: Future<Output = T>,
{
    let timeout_handle = tokio::time::timeout(timeout, action);
    match timeout_handle.await {
        Ok(res) => res,
        Err(_) => panic!("timed out waiting for metadata calculator"),
    }
}

pub(super) fn mock_config(db_path: &Path) -> MetadataCalculatorConfig {
    MetadataCalculatorConfig {
        db_path: db_path.to_str().unwrap().to_owned(),
        max_open_files: None,
        mode: MerkleTreeMode::Full,
        delay_interval: POLL_INTERVAL,
        max_l1_batches_per_iter: 10,
        multi_get_chunk_size: 500,
        block_cache_capacity: 0,
        include_indices_and_filters_in_block_cache: false,
        memtable_capacity: 16 << 20,            // 16 MiB
        stalled_writes_timeout: Duration::ZERO, // writes should never be stalled in tests
        recovery: MetadataCalculatorRecoveryConfig::default(),
    }
}

#[tokio::test]
async fn genesis_creation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, _) = setup_calculator(temp_dir.path(), pool.clone()).await;
    run_calculator(calculator).await;
    let (calculator, _) = setup_calculator(temp_dir.path(), pool).await;

    let tree = calculator.create_tree().await.unwrap();
    let GenericAsyncTree::Ready(tree) = tree else {
        panic!("Unexpected tree state: {tree:?}");
    };
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(1));
}

#[tokio::test]
async fn low_level_genesis_creation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    insert_genesis_batch(
        &mut pool.connection().await.unwrap(),
        &GenesisParams::mock(),
    )
    .await
    .unwrap();
    reset_db_state(&pool, 1).await;

    let db = RocksDB::new(temp_dir.path()).unwrap();
    let mut tree = AsyncTree::new(db.into(), MerkleTreeMode::Lightweight).unwrap();
    let (_stop_sender, mut stop_receiver) = watch::channel(false);
    tree.ensure_consistency(&Delayer::new(POLL_INTERVAL), &pool, &mut stop_receiver)
        .await
        .unwrap();

    assert!(!tree.is_empty());
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(1));
}

#[test_casing(8, Product(([1, 4, 7, 9], [false, true])))]
#[tokio::test]
async fn tree_truncation_on_l1_batch_divergence(
    last_common_l1_batch: u32,
    overwrite_tree_data: bool,
) {
    const INITIAL_BATCH_COUNT: usize = 10;

    assert!((last_common_l1_batch as usize) < INITIAL_BATCH_COUNT);
    let last_common_l1_batch = L1BatchNumber(last_common_l1_batch);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, INITIAL_BATCH_COUNT).await;
    run_calculator(calculator).await;

    let mut storage = pool.connection().await.unwrap();
    remove_l1_batches(&mut storage, last_common_l1_batch).await;
    // Extend the state with new L1 batches.
    let logs = gen_storage_logs(100..200, 5);
    extend_db_state(&mut storage, logs).await;

    if overwrite_tree_data {
        for number in (last_common_l1_batch.0 + 1)..(last_common_l1_batch.0 + 6) {
            let new_tree_data = L1BatchTreeData {
                hash: H256::from_low_u64_be(number.into()),
                rollup_last_leaf_index: 200, // doesn't matter
            };
            storage
                .blocks_dal()
                .save_l1_batch_tree_data(L1BatchNumber(number), &new_tree_data)
                .await
                .unwrap();
        }
    }

    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    let tree = calculator.create_tree().await.unwrap();
    let GenericAsyncTree::Ready(mut tree) = tree else {
        panic!("Unexpected tree state: {tree:?}");
    };
    assert_eq!(
        tree.next_l1_batch_number(),
        L1BatchNumber(INITIAL_BATCH_COUNT as u32 + 1)
    );

    let (_stop_sender, mut stop_receiver) = watch::channel(false);
    tree.ensure_consistency(&Delayer::new(POLL_INTERVAL), &pool, &mut stop_receiver)
        .await
        .unwrap();
    assert_eq!(tree.next_l1_batch_number(), last_common_l1_batch + 1);
}

#[test_casing(4, [1, 4, 6, 7])]
#[tokio::test]
async fn tree_truncation_on_l1_batch_divergence_in_pruned_tree(retained_l1_batch: u32) {
    const INITIAL_BATCH_COUNT: usize = 10;
    const LAST_COMMON_L1_BATCH: L1BatchNumber = L1BatchNumber(6);

    let retained_l1_batch = L1BatchNumber(retained_l1_batch);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, INITIAL_BATCH_COUNT).await;
    run_calculator(calculator).await;

    let mut storage = pool.connection().await.unwrap();
    remove_l1_batches(&mut storage, LAST_COMMON_L1_BATCH).await;
    // Extend the state with new L1 batches.
    let logs = gen_storage_logs(100..200, 5);
    extend_db_state(&mut storage, logs).await;

    for number in (LAST_COMMON_L1_BATCH.0 + 1)..(LAST_COMMON_L1_BATCH.0 + 6) {
        let new_tree_data = L1BatchTreeData {
            hash: H256::from_low_u64_be(number.into()),
            rollup_last_leaf_index: 200, // doesn't matter
        };
        storage
            .blocks_dal()
            .save_l1_batch_tree_data(L1BatchNumber(number), &new_tree_data)
            .await
            .unwrap();
    }

    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    let tree = calculator.create_tree().await.unwrap();
    let GenericAsyncTree::Ready(mut tree) = tree else {
        panic!("Unexpected tree state: {tree:?}");
    };

    let reader = tree.reader();
    let (mut pruner, pruner_handle) = tree.pruner();
    pruner.set_poll_interval(POLL_INTERVAL);
    tokio::task::spawn_blocking(|| pruner.run());
    pruner_handle
        .set_target_retained_version(retained_l1_batch.0.into())
        .unwrap();
    // Wait until the tree is pruned
    while reader.clone().info().await.min_l1_batch_number < Some(retained_l1_batch) {
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    let (_stop_sender, mut stop_receiver) = watch::channel(false);
    let consistency_result = tree
        .ensure_consistency(&Delayer::new(POLL_INTERVAL), &pool, &mut stop_receiver)
        .await;

    if retained_l1_batch <= LAST_COMMON_L1_BATCH {
        consistency_result.unwrap();
        assert_eq!(tree.next_l1_batch_number(), LAST_COMMON_L1_BATCH + 1);
    } else {
        let err = consistency_result.unwrap_err();
        assert!(
            format!("{err:#}").contains("diverging min L1 batch"),
            "{err:#}"
        );
    }
}

#[tokio::test]
async fn basic_workflow() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, object_store) = setup_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, 1).await;
    let merkle_tree_hash = run_calculator(calculator).await;

    // Check the hash against the reference.
    let expected_tree_hash = expected_tree_hash(&pool).await;
    assert_eq!(merkle_tree_hash, expected_tree_hash);

    let job: PrepareBasicCircuitsJob = object_store.get(L1BatchNumber(1)).await.unwrap();
    assert!(job.next_enumeration_index() > 0);
    let merkle_paths: Vec<_> = job.clone().into_merkle_paths().collect();
    assert!(!merkle_paths.is_empty() && merkle_paths.len() <= 100);
    // ^ The exact values depend on ops in genesis block
    assert!(merkle_paths.iter().all(|log| log.is_write));

    let (calculator, _) = setup_calculator(temp_dir.path(), pool).await;
    let tree = calculator.create_tree().await.unwrap();
    let GenericAsyncTree::Ready(tree) = tree else {
        panic!("Unexpected tree state: {tree:?}");
    };
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(2));
}

async fn expected_tree_hash(pool: &ConnectionPool<Core>) -> H256 {
    let mut storage = pool.connection().await.unwrap();
    let sealed_l1_batch_number = storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("No L1 batches in Postgres");
    let mut all_logs = vec![];
    for i in 0..=sealed_l1_batch_number.0 {
        let logs =
            L1BatchWithLogs::new(&mut storage, L1BatchNumber(i), MerkleTreeMode::Lightweight)
                .await
                .unwrap();
        let logs = logs.expect("no L1 batch").storage_logs;

        all_logs.extend(logs);
    }
    ZkSyncTree::process_genesis_batch(&all_logs).root_hash
}

#[tokio::test]
async fn status_receiver_has_correct_states() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (mut calculator, _) = setup_calculator(temp_dir.path(), pool.clone()).await;
    let tree_health_check = calculator.tree_health_check();
    assert_eq!(tree_health_check.name(), "tree");
    let health = tree_health_check.check_health().await;
    assert_matches!(health.status(), HealthStatus::NotReady);

    let other_tree_health_check = calculator.tree_health_check();
    assert_eq!(other_tree_health_check.name(), "tree");
    let health = other_tree_health_check.check_health().await;
    assert_matches!(health.status(), HealthStatus::NotReady);

    reset_db_state(&pool, 1).await;
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let calculator_handle = tokio::spawn(calculator.run(stop_rx));
    delay_rx.recv().await.unwrap();
    assert_eq!(
        tree_health_check.check_health().await.status(),
        HealthStatus::Ready
    );
    assert_eq!(
        other_tree_health_check.check_health().await.status(),
        HealthStatus::Ready
    );

    stop_sx.send(true).unwrap();
    tokio::time::timeout(RUN_TIMEOUT, calculator_handle)
        .await
        .expect("timed out waiting for calculator")
        .unwrap()
        .unwrap();
    assert_eq!(
        tree_health_check.check_health().await.status(),
        HealthStatus::ShutDown
    );
    assert_eq!(
        other_tree_health_check.check_health().await.status(),
        HealthStatus::ShutDown
    );

    // Check that health checks don't prevent dropping RocksDB instances.
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
}

#[tokio::test]
async fn multi_l1_batch_workflow() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    // Collect all storage logs in a single L1 batch
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, 1).await;
    let root_hash = run_calculator(calculator).await;

    // Collect the same logs in multiple L1 batches
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, object_store) = setup_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, 10).await;
    let multi_block_root_hash = run_calculator(calculator).await;
    assert_eq!(multi_block_root_hash, root_hash);

    let mut prev_index = None;
    for l1_batch_number in 1..=10 {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let job: PrepareBasicCircuitsJob = object_store.get(l1_batch_number).await.unwrap();
        let next_enumeration_index = job.next_enumeration_index();
        let merkle_paths: Vec<_> = job.into_merkle_paths().collect();
        assert!(!merkle_paths.is_empty() && merkle_paths.len() <= 10);

        if let Some(prev_index) = prev_index {
            assert_eq!(next_enumeration_index, prev_index + 1);
        }
        let max_leaf_index_in_block = merkle_paths
            .iter()
            .filter_map(|log| log.first_write.then_some(log.leaf_enumeration_index))
            .max();
        prev_index = max_leaf_index_in_block.or(prev_index);
    }
}

#[tokio::test]
async fn running_metadata_calculator_with_additional_blocks() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, 5).await;
    run_calculator(calculator).await;

    let mut calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let calculator_handle = tokio::spawn(calculator.run(stop_rx));
    // Wait until the calculator has processed initial L1 batches.
    let (next_l1_batch, _) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
        .await
        .expect("metadata calculator timed out processing initial blocks")
        .unwrap();
    assert_eq!(next_l1_batch, L1BatchNumber(6));

    // Add some new blocks to the storage.
    let new_logs = gen_storage_logs(100..200, 10);
    extend_db_state(&mut pool.connection().await.unwrap(), new_logs).await;

    // Wait until these blocks are processed. The calculator may have spurious delays,
    // thus we wait in a loop.
    let updated_root_hash = loop {
        let (next_l1_batch, root_hash) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
            .await
            .expect("metadata calculator shut down prematurely")
            .unwrap();
        if next_l1_batch == L1BatchNumber(16) {
            stop_sx.send(true).unwrap(); // Shut down the calculator.
            break root_hash;
        }
    };
    tokio::time::timeout(RUN_TIMEOUT, calculator_handle)
        .await
        .expect("timed out waiting for calculator")
        .unwrap()
        .unwrap();

    // Switch to the full tree. It should pick up from the same spot and result in the same tree root hash.
    let (calculator, _) = setup_calculator(temp_dir.path(), pool).await;
    let root_hash_for_full_tree = run_calculator(calculator).await;
    assert_eq!(root_hash_for_full_tree, updated_root_hash);
}

#[tokio::test]
async fn shutting_down_calculator() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (merkle_tree_config, mut operation_config) =
        create_config(temp_dir.path(), MerkleTreeMode::Lightweight);
    operation_config.delay_interval = 30_000; // ms; chosen to be larger than `RUN_TIMEOUT`

    let calculator =
        setup_calculator_with_options(&merkle_tree_config, &operation_config, pool.clone(), None)
            .await;

    reset_db_state(&pool, 5).await;

    let (stop_sx, stop_rx) = watch::channel(false);
    let calculator_task = tokio::spawn(calculator.run(stop_rx));
    tokio::time::sleep(POLL_INTERVAL).await;
    stop_sx.send_replace(true);
    run_with_timeout(RUN_TIMEOUT, calculator_task)
        .await
        .unwrap()
        .unwrap();
}

async fn test_postgres_backup_recovery(
    sleep_between_batches: bool,
    insert_batch_without_metadata: bool,
) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, 5).await;
    run_calculator(calculator).await;

    // Simulate recovery from a DB snapshot in which some newer L1 batches are erased.
    let last_batch_after_recovery = L1BatchNumber(3);
    let mut storage = pool.connection().await.unwrap();
    let removed_batches = remove_l1_batches(&mut storage, last_batch_after_recovery).await;

    if insert_batch_without_metadata {
        let batches_without_metadata =
            remove_l1_batches(&mut storage, last_batch_after_recovery - 1).await;
        let [batch_without_metadata] = batches_without_metadata.as_slice() else {
            unreachable!()
        };
        // Re-insert the last batch without metadata immediately.
        storage
            .blocks_dal()
            .insert_mock_l1_batch(batch_without_metadata)
            .await
            .unwrap();
        insert_initial_writes_for_batch(&mut storage, batch_without_metadata.number).await;
    }
    drop(storage);

    let mut calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let calculator_handle = tokio::spawn(calculator.run(stop_rx));
    // Wait until the calculator has processed initial L1 batches.
    let (next_l1_batch, _) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
        .await
        .expect("metadata calculator timed out after recovery")
        .unwrap();
    assert_eq!(next_l1_batch, last_batch_after_recovery + 1);

    // Re-insert L1 batches to the storage after recovery.
    let mut storage = pool.connection().await.unwrap();
    for batch_header in &removed_batches {
        let mut txn = storage.start_transaction().await.unwrap();
        txn.blocks_dal()
            .insert_mock_l1_batch(batch_header)
            .await
            .unwrap();
        insert_initial_writes_for_batch(&mut txn, batch_header.number).await;
        txn.commit().await.unwrap();
        if sleep_between_batches {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
    drop(storage);

    // Wait until these batches are processed.
    loop {
        let (next_l1_batch, _) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
            .await
            .expect("metadata calculator shut down prematurely")
            .unwrap();
        if next_l1_batch == L1BatchNumber(6) {
            stop_sx.send(true).unwrap(); // Shut down the calculator.
            break;
        }
    }
    tokio::time::timeout(RUN_TIMEOUT, calculator_handle)
        .await
        .expect("timed out waiting for calculator")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn postgres_backup_recovery() {
    test_postgres_backup_recovery(false, false).await;
}

#[tokio::test]
async fn postgres_backup_recovery_with_delay_between_batches() {
    test_postgres_backup_recovery(true, false).await;
}

#[tokio::test]
async fn postgres_backup_recovery_with_excluded_metadata() {
    test_postgres_backup_recovery(false, true).await;
}

pub(crate) async fn setup_calculator(
    db_path: &Path,
    pool: ConnectionPool<Core>,
) -> (MetadataCalculator, Arc<dyn ObjectStore>) {
    let store = MockObjectStore::arc();
    let (merkle_tree_config, operation_manager) = create_config(db_path, MerkleTreeMode::Full);
    let calculator = setup_calculator_with_options(
        &merkle_tree_config,
        &operation_manager,
        pool,
        Some(store.clone()),
    )
    .await;
    (calculator, store)
}

async fn setup_lightweight_calculator(
    db_path: &Path,
    pool: ConnectionPool<Core>,
) -> MetadataCalculator {
    let (db_config, operation_config) = create_config(db_path, MerkleTreeMode::Lightweight);
    setup_calculator_with_options(&db_config, &operation_config, pool, None).await
}

fn create_config(
    db_path: &Path,
    mode: MerkleTreeMode,
) -> (MerkleTreeConfig, OperationsManagerConfig) {
    let db_config = MerkleTreeConfig {
        path: path_to_string(&db_path.join("new")),
        mode,
        ..MerkleTreeConfig::default()
    };

    let operation_config = OperationsManagerConfig {
        delay_interval: 50, // ms
    };
    (db_config, operation_config)
}

async fn setup_calculator_with_options(
    merkle_tree_config: &MerkleTreeConfig,
    operation_config: &OperationsManagerConfig,
    pool: ConnectionPool<Core>,
    object_store: Option<Arc<dyn ObjectStore>>,
) -> MetadataCalculator {
    let mut storage = pool.connection().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
    }
    drop(storage);

    let calculator_config =
        MetadataCalculatorConfig::for_main_node(merkle_tree_config, operation_config);
    MetadataCalculator::new(calculator_config, object_store, pool)
        .await
        .unwrap()
}

fn path_to_string(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

pub(crate) async fn run_calculator(mut calculator: MetadataCalculator) -> H256 {
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;
    let delayer_handle = tokio::spawn(async move {
        // Wait until the calculator has processed all initially available L1 batches,
        // then stop it via signal.
        let (_, root_hash) = delay_rx
            .recv()
            .await
            .expect("metadata calculator shut down prematurely");
        stop_sx.send(true).unwrap();
        root_hash
    });

    run_with_timeout(RUN_TIMEOUT, calculator.run(stop_rx))
        .await
        .unwrap();
    delayer_handle.await.unwrap()
}

pub(crate) async fn reset_db_state(pool: &ConnectionPool<Core>, num_batches: usize) {
    let mut storage = pool.connection().await.unwrap();
    // Drops all L1 batches (except the L1 batch with number 0) and their storage logs.
    storage
        .storage_logs_dal()
        .roll_back_storage_logs(L2BlockNumber(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l2_blocks(L2BlockNumber(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l1_batches(L1BatchNumber(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_initial_writes(L1BatchNumber(0))
        .await
        .unwrap();

    let logs = gen_storage_logs(0..100, num_batches);
    extend_db_state(&mut storage, logs).await;
}

pub(super) async fn extend_db_state(
    storage: &mut Connection<'_, Core>,
    new_logs: impl IntoIterator<Item = Vec<StorageLog>>,
) {
    let mut storage = storage.start_transaction().await.unwrap();
    let sealed_l1_batch = storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("no L1 batches in Postgres");
    extend_db_state_from_l1_batch(&mut storage, sealed_l1_batch + 1, new_logs).await;
    storage.commit().await.unwrap();
}

pub(super) async fn extend_db_state_from_l1_batch(
    storage: &mut Connection<'_, Core>,
    next_l1_batch: L1BatchNumber,
    new_logs: impl IntoIterator<Item = Vec<StorageLog>>,
) {
    assert!(storage.in_transaction(), "must be called in DB transaction");

    for (idx, batch_logs) in (next_l1_batch.0..).zip(new_logs) {
        let header = create_l1_batch(idx);
        let batch_number = header.number;
        // Assumes that L1 batch consists of only one L2 block.
        let l2_block_header = create_l2_block(idx);
        let l2_block_number = l2_block_header.number;

        storage
            .blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();
        storage
            .storage_logs_dal()
            .insert_storage_logs(l2_block_number, &[(H256::zero(), batch_logs)])
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(batch_number)
            .await
            .unwrap();
        insert_initial_writes_for_batch(storage, batch_number).await;
    }
}

async fn insert_initial_writes_for_batch(
    connection: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
) {
    let written_non_zero_slots: Vec<_> = connection
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(l1_batch_number)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(key, value)| (!value.is_zero()).then_some(key))
        .collect();
    let hashed_keys: Vec<_> = written_non_zero_slots
        .iter()
        .map(|key| key.hashed_key())
        .collect();
    let pre_written_slots = connection
        .storage_logs_dedup_dal()
        .filter_written_slots(&hashed_keys)
        .await
        .unwrap();

    let keys_to_insert: Vec<_> = written_non_zero_slots
        .into_iter()
        .sorted()
        .filter(|key| !pre_written_slots.contains(&key.hashed_key()))
        .collect();
    connection
        .storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &keys_to_insert)
        .await
        .unwrap();
}

pub(crate) fn gen_storage_logs(
    indices: ops::Range<u32>,
    num_batches: usize,
) -> Vec<Vec<StorageLog>> {
    // Addresses and keys of storage logs must be sorted for the `multi_block_workflow` test.
    let mut accounts = [
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .map(|s| AccountTreeId::new(s.parse::<Address>().unwrap()));
    accounts.sort_unstable();

    let account_keys = (indices.start / 5)..(indices.end / 5);
    let proof_keys = accounts.iter().flat_map(|&account| {
        account_keys
            .clone()
            .map(move |i| StorageKey::new(account, u32_to_h256(i)))
    });
    let proof_values = indices.map(u32_to_h256);

    let logs: Vec<_> = proof_keys
        .zip(proof_values)
        .map(|(proof_key, proof_value)| StorageLog::new_write_log(proof_key, proof_value))
        .collect();
    for window in logs.windows(2) {
        let [prev, next] = window else { unreachable!() };
        assert!(prev.key < next.key);
    }

    logs.chunks(logs.len() / num_batches)
        .map(<[_]>::to_vec)
        .collect()
}

async fn remove_l1_batches(
    storage: &mut Connection<'_, Core>,
    last_l1_batch_to_keep: L1BatchNumber,
) -> Vec<L1BatchHeader> {
    let sealed_l1_batch_number = storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("no L1 batches in Postgres");
    assert!(sealed_l1_batch_number >= last_l1_batch_to_keep);

    let mut batch_headers = vec![];
    for batch_number in (last_l1_batch_to_keep.0 + 1)..=sealed_l1_batch_number.0 {
        let header = storage
            .blocks_dal()
            .get_l1_batch_header(L1BatchNumber(batch_number))
            .await
            .unwrap();
        batch_headers.push(header.unwrap());
    }

    let (_, last_l2_block_to_keep) = storage
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(last_l1_batch_to_keep)
        .await
        .unwrap()
        .expect("L1 batch has no blocks");

    storage
        .storage_logs_dal()
        .roll_back_storage_logs(last_l2_block_to_keep)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l2_blocks(last_l2_block_to_keep)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l1_batches(last_l1_batch_to_keep)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_initial_writes(last_l1_batch_to_keep)
        .await
        .unwrap();
    batch_headers
}

#[tokio::test]
async fn deduplication_works_as_expected() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let logs = gen_storage_logs(100..120, 1).pop().unwrap();
    let hashed_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
    extend_db_state(&mut storage, [logs.clone()]).await;

    let initial_writes = storage
        .storage_logs_dal()
        .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
        .await
        .unwrap();
    assert_eq!(initial_writes.len(), hashed_keys.len());
    assert!(initial_writes
        .values()
        .all(|&(batch, _)| batch == L1BatchNumber(1)));

    let mut new_logs = gen_storage_logs(120..140, 1).pop().unwrap();
    let new_hashed_keys: Vec<_> = new_logs.iter().map(|log| log.key.hashed_key()).collect();
    let updated_logs = logs.into_iter().step_by(2).map(|mut log| {
        log.value = H256::zero();
        log
    });
    new_logs.extend(updated_logs);
    extend_db_state(&mut storage, [new_logs]).await;

    // Initial writes for previously inserted keys should not change.
    let initial_writes = storage
        .storage_logs_dal()
        .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
        .await
        .unwrap();
    assert_eq!(initial_writes.len(), hashed_keys.len());
    assert!(initial_writes
        .values()
        .all(|&(batch, _)| batch == L1BatchNumber(1)));

    let initial_writes = storage
        .storage_logs_dal()
        .get_l1_batches_and_indices_for_initial_writes(&new_hashed_keys)
        .await
        .unwrap();
    assert_eq!(initial_writes.len(), new_hashed_keys.len());
    assert!(initial_writes
        .values()
        .all(|&(batch, _)| batch == L1BatchNumber(2)));

    let mut no_op_logs = gen_storage_logs(140..160, 1).pop().unwrap();
    let no_op_hashed_keys: Vec<_> = no_op_logs.iter().map(|log| log.key.hashed_key()).collect();
    for log in &mut no_op_logs {
        log.value = H256::zero();
    }
    extend_db_state(&mut storage, [no_op_logs.clone()]).await;

    let initial_writes = storage
        .storage_logs_dal()
        .get_l1_batches_and_indices_for_initial_writes(&no_op_hashed_keys)
        .await
        .unwrap();
    assert!(initial_writes.is_empty());

    let updated_logs: Vec<_> = no_op_logs
        .iter()
        .step_by(2)
        .map(|log| StorageLog {
            value: H256::repeat_byte(0x11),
            ..*log
        })
        .collect();
    no_op_logs.extend_from_slice(&updated_logs);
    extend_db_state(&mut storage, [no_op_logs]).await;

    let initial_writes = storage
        .storage_logs_dal()
        .get_l1_batches_and_indices_for_initial_writes(&no_op_hashed_keys)
        .await
        .unwrap();
    assert_eq!(initial_writes.len(), no_op_hashed_keys.len() / 2);
    for key in no_op_hashed_keys.iter().step_by(2) {
        assert_eq!(initial_writes[key].0, L1BatchNumber(4));
    }
}

#[test_casing(3, [3, 5, 8])]
#[tokio::test]
async fn l1_batch_divergence_entire_workflow(last_common_l1_batch: u32) {
    const INITIAL_BATCH_COUNT: usize = 10;

    assert!((last_common_l1_batch as usize) < INITIAL_BATCH_COUNT);
    let last_common_l1_batch = L1BatchNumber(last_common_l1_batch);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    reset_db_state(&pool, INITIAL_BATCH_COUNT).await;
    run_calculator(calculator).await;

    let mut storage = pool.connection().await.unwrap();
    remove_l1_batches(&mut storage, last_common_l1_batch).await;
    // Extend the state with new L1 batches.
    let logs = gen_storage_logs(100..200, 5);
    extend_db_state(&mut storage, logs).await;
    let expected_root_hash = expected_tree_hash(&pool).await;

    let calculator = setup_lightweight_calculator(temp_dir.path(), pool.clone()).await;
    let final_root_hash = run_calculator(calculator).await;
    assert_eq!(final_root_hash, expected_root_hash);
}

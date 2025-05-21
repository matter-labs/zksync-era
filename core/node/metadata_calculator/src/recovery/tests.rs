//! Tests for metadata calculator snapshot recovery.

use std::{path::Path, sync::Mutex, time::Duration};

use assert_matches::assert_matches;
use tempfile::TempDir;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_config::configs::{
    chain::{OperationsManagerConfig, StateKeeperConfig},
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::{
    custom_genesis_export_dal::{GenesisState, StorageLogRow},
    CoreDal,
};
use zksync_health_check::{CheckHealth, HealthStatus, ReactiveHealthCheck};
use zksync_merkle_tree::{domain::ZkSyncTree, recovery::PersistenceThreadHandle, TreeInstruction};
use zksync_node_genesis::{
    insert_genesis_batch, insert_genesis_batch_with_custom_state, GenesisParams,
};
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_storage::RocksDB;
use zksync_types::{L1BatchNumber, StorageLog};

use super::*;
use crate::{
    helpers::create_db,
    tests::{
        extend_db_state, extend_db_state_from_l1_batch, gen_storage_logs, mock_config,
        run_calculator, setup_calculator,
    },
    MetadataCalculator, MetadataCalculatorConfig,
};

impl HandleRecoveryEvent for () {}

#[test]
fn calculating_chunk_count() {
    let mut snapshot = InitParameters {
        l1_batch: L1BatchNumber(1),
        l2_block: L2BlockNumber(1),
        log_count: 160_000_000,
        expected_root_hash: Some(H256::zero()),
        desired_chunk_size: 200_000,
    };
    assert_eq!(snapshot.chunk_count(), 800);

    snapshot.log_count += 1;
    assert_eq!(snapshot.chunk_count(), 801);

    snapshot.log_count = 100;
    assert_eq!(snapshot.chunk_count(), 1);
}

async fn create_tree_recovery(
    path: &Path,
    l1_batch: L1BatchNumber,
    config: &MetadataCalculatorRecoveryConfig,
) -> (AsyncTreeRecovery, Option<PersistenceThreadHandle>) {
    let db = create_db(mock_config(path)).await.unwrap();
    AsyncTreeRecovery::with_handle(db, l1_batch.0.into(), MerkleTreeMode::Full, config).unwrap()
}

#[tokio::test]
async fn basic_recovery_workflow() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let root_hash = prepare_storage_logs(pool.clone(), &temp_dir).await;
    prune_storage(&pool, L1BatchNumber(1)).await;

    let config = MetadataCalculatorRecoveryConfig::default();
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");
    assert!(init_params.log_count > 200, "{init_params:?}");

    let (_stop_sender, stop_receiver) = watch::channel(false);
    for chunk_count in [1, 4, 9, 16, 60, 256] {
        println!("Recovering tree with {chunk_count} chunks");

        let tree_path = temp_dir.path().join(format!("recovery-{chunk_count}"));
        let (tree, _) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
        let (health_check, health_updater) = ReactiveHealthCheck::new("tree");
        let recovery_options = RecoveryOptions {
            chunk_count,
            concurrency_limit: 1,
            events: Box::new(RecoveryHealthUpdater::new(&health_updater)),
        };
        let tree = tree
            .recover(init_params, recovery_options, &pool, &stop_receiver)
            .await
            .unwrap();

        assert_eq!(tree.root_hash(), root_hash);
        let health = health_check.check_health().await;
        assert_matches!(health.status(), HealthStatus::Affected);
    }
}

async fn prepare_storage_logs(pool: ConnectionPool<Core>, temp_dir: &TempDir) -> H256 {
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let logs = gen_storage_logs(100..300, 1).pop().unwrap();
    extend_db_state(&mut storage, vec![logs]).await;
    drop(storage);

    // Ensure that metadata for L1 batch #1 is present in the DB.
    let (calculator, _) = setup_calculator(&temp_dir.path().join("init"), pool, true).await;
    run_calculator(calculator).await
}

async fn prune_storage(pool: &ConnectionPool<Core>, pruned_l1_batch: L1BatchNumber) {
    // Emulate pruning batches in the storage.
    let mut storage = pool.connection().await.unwrap();
    let (_, pruned_l2_block) = storage
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(pruned_l1_batch)
        .await
        .unwrap()
        .expect("L1 batch not present in Postgres");
    let root_hash = storage
        .blocks_dal()
        .get_l1_batch_state_root(pruned_l1_batch)
        .await
        .unwrap()
        .expect("L1 batch does not have root hash");

    storage
        .pruning_dal()
        .insert_soft_pruning_log(pruned_l1_batch, pruned_l2_block)
        .await
        .unwrap();
    let pruning_stats = storage
        .pruning_dal()
        .hard_prune_batches_range(pruned_l1_batch, pruned_l2_block)
        .await
        .unwrap();
    assert!(
        pruning_stats.deleted_l1_batches > 0 && pruning_stats.deleted_l2_blocks > 0,
        "{pruning_stats:?}"
    );
    storage
        .pruning_dal()
        .insert_hard_pruning_log(pruned_l1_batch, pruned_l2_block, root_hash)
        .await
        .unwrap();
}

#[tokio::test]
async fn recovery_workflow_for_partial_pruning() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let recovery_root_hash = prepare_storage_logs(pool.clone(), &temp_dir).await;

    // Add more storage logs and prune initial logs.
    let logs = gen_storage_logs(200..400, 5);
    extend_db_state(&mut pool.connection().await.unwrap(), logs).await;
    let (calculator, _) = setup_calculator(&temp_dir.path().join("init"), pool.clone(), true).await;
    let final_root_hash = run_calculator(calculator).await;
    prune_storage(&pool, L1BatchNumber(1)).await;

    let tree_path = temp_dir.path().join("recovery");
    let db = create_db(mock_config(&tree_path)).await.unwrap();
    let tree = GenericAsyncTree::Empty {
        db,
        mode: MerkleTreeMode::Lightweight,
    };
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let tree = tree
        .ensure_ready(
            &MetadataCalculatorRecoveryConfig::default(),
            &pool,
            pool.clone(),
            &ReactiveHealthCheck::new("tree").1,
            &stop_receiver,
        )
        .await
        .unwrap();

    assert_eq!(tree.root_hash(), recovery_root_hash);
    drop(tree); // Release exclusive lock on RocksDB

    // Check that tree operates as intended after recovery
    let (calculator, _) = setup_calculator(&tree_path, pool, true).await;
    assert_eq!(run_calculator(calculator).await, final_root_hash);
}

#[derive(Debug)]
struct TestEventListener {
    expected_recovered_chunks: u64,
    stop_threshold: u64,
    persistence_handle: Mutex<Option<(PersistenceThreadHandle, u64)>>,
    processed_chunk_count: AtomicU64,
    stop_sender: watch::Sender<bool>,
}

impl TestEventListener {
    fn new(stop_threshold: u64, stop_sender: watch::Sender<bool>) -> Self {
        Self {
            expected_recovered_chunks: 0,
            stop_threshold,
            persistence_handle: Mutex::default(),
            processed_chunk_count: AtomicU64::new(0),
            stop_sender,
        }
    }

    fn expect_recovered_chunks(mut self, count: u64) -> Self {
        self.expected_recovered_chunks = count;
        self
    }

    fn crash_persistence_after(
        mut self,
        chunk_count: u64,
        handle: PersistenceThreadHandle,
    ) -> Self {
        assert!(chunk_count < self.stop_threshold);
        self.persistence_handle = Mutex::new(Some((handle, chunk_count)));
        self
    }
}

#[async_trait]
impl HandleRecoveryEvent for TestEventListener {
    fn recovery_started(&mut self, _chunk_count: u64, recovered_chunk_count: u64) {
        assert_eq!(recovered_chunk_count, self.expected_recovered_chunks);
    }

    async fn chunk_recovered(&self) {
        let processed_chunk_count = self.processed_chunk_count.fetch_add(1, Ordering::SeqCst) + 1;
        if processed_chunk_count >= self.stop_threshold {
            self.stop_sender.send_replace(true);
        }

        let mut persistence_handle = self.persistence_handle.lock().unwrap();
        if let Some((_, crash_threshold)) = &*persistence_handle {
            if processed_chunk_count >= *crash_threshold {
                let (handle, _) = persistence_handle.take().unwrap();
                handle.test_stop_processing();
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FaultToleranceCase {
    Sequential,
    Parallel,
    ParallelWithCrash,
}

impl FaultToleranceCase {
    const ALL: [Self; 3] = [Self::Sequential, Self::Parallel, Self::ParallelWithCrash];
}

#[test_casing(9, Product(([5, 7, 8], FaultToleranceCase::ALL)))]
#[tokio::test]
async fn recovery_fault_tolerance(chunk_count: u64, case: FaultToleranceCase) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let root_hash = prepare_storage_logs(pool.clone(), &temp_dir).await;
    prune_storage(&pool, L1BatchNumber(1)).await;

    let tree_path = temp_dir.path().join("recovery");
    let mut config = MetadataCalculatorRecoveryConfig::default();
    assert!(config.parallel_persistence_buffer.is_some());
    if matches!(case, FaultToleranceCase::Sequential) {
        config.parallel_persistence_buffer = None;
    }

    let (tree, _) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(TestEventListener::new(1, stop_sender)),
    };
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");
    let err = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap_err();
    assert_matches!(err, OrStopped::Stopped);

    // Emulate a restart and recover 2 more chunks (or 1 + emulated persistence crash).
    let (mut tree, handle) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    assert_ne!(tree.root_hash().await, root_hash);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut event_listener = TestEventListener::new(2, stop_sender).expect_recovered_chunks(1);
    let expected_recovered_chunks = if matches!(case, FaultToleranceCase::ParallelWithCrash) {
        event_listener = event_listener.crash_persistence_after(1, handle.unwrap());
        2
    } else {
        drop(handle); // necessary to terminate the background persistence thread in time
        3
    };
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(event_listener),
    };
    let recovery_result = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await;
    if matches!(case, FaultToleranceCase::ParallelWithCrash) {
        let err = format!("{:#}", recovery_result.unwrap_err());
        assert!(err.contains("emulated persistence crash"), "{err}");
    } else {
        assert_matches!(recovery_result.unwrap_err(), OrStopped::Stopped);
    }

    // Emulate another restart and recover remaining chunks.
    let (mut tree, _) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    assert_ne!(tree.root_hash().await, root_hash);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(
            TestEventListener::new(u64::MAX, stop_sender)
                .expect_recovered_chunks(expected_recovered_chunks),
        ),
    };
    let tree = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap();
    assert_eq!(tree.root_hash(), root_hash);
}

#[derive(Debug)]
enum RecoveryWorkflowCase {
    Stop,
    CreateBatch,
}

impl RecoveryWorkflowCase {
    const ALL: [Self; 2] = [Self::Stop, Self::CreateBatch];
}

#[test_casing(2, RecoveryWorkflowCase::ALL)]
#[tokio::test]
async fn entire_recovery_workflow(case: RecoveryWorkflowCase) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    // Emulate the recovered view of Postgres. Unlike with previous tests, we don't perform genesis.
    let snapshot_logs = gen_storage_logs(100..300, 1).pop().unwrap();
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery = prepare_recovery_snapshot(
        &mut storage,
        L1BatchNumber(23),
        L2BlockNumber(42),
        &snapshot_logs,
    )
    .await;

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let merkle_tree_config = MerkleTreeConfig::for_tests(temp_dir.path().to_owned());
    let calculator_config = MetadataCalculatorConfig::for_main_node(
        &merkle_tree_config,
        &OperationsManagerConfig {
            delay_interval: Duration::from_millis(50),
        },
        &StateKeeperConfig {
            protective_reads_persistence_enabled: true,
            ..StateKeeperConfig::for_tests()
        },
    );
    let mut calculator = MetadataCalculator::new(calculator_config, None, pool.clone())
        .await
        .unwrap();
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let tree_reader = calculator.tree_reader();
    let calculator_task = tokio::spawn(calculator.run(stop_receiver));

    match case {
        // Wait until the tree is fully initialized and stop the calculator.
        RecoveryWorkflowCase::Stop => {
            let tree_info = tree_reader.wait().await.unwrap().info().await;
            assert_eq!(tree_info.root_hash, snapshot_recovery.l1_batch_root_hash);
            assert_eq!(tree_info.leaf_count, 200);
            assert_eq!(
                tree_info.next_l1_batch_number,
                snapshot_recovery.l1_batch_number + 1
            );
        }

        // Emulate state keeper adding a new L1 batch to Postgres.
        RecoveryWorkflowCase::CreateBatch => {
            tree_reader.wait().await.unwrap();

            let mut storage = storage.start_transaction().await.unwrap();
            let mut new_logs = gen_storage_logs(500..600, 1).pop().unwrap();
            // Logs must be sorted by `log.key` to match their enum index assignment
            new_logs.sort_unstable_by_key(|log| log.key);

            extend_db_state_from_l1_batch(
                &mut storage,
                snapshot_recovery.l1_batch_number + 1,
                snapshot_recovery.l2_block_number + 1,
                [new_logs.clone()],
            )
            .await;
            storage.commit().await.unwrap();

            // Wait until the inserted L1 batch is processed by the calculator.
            let new_root_hash = loop {
                let (next_l1_batch, root_hash) = delay_rx.recv().await.unwrap();
                if next_l1_batch == snapshot_recovery.l1_batch_number + 2 {
                    break root_hash;
                }
            };

            let all_tree_instructions: Vec<_> = snapshot_logs
                .iter()
                .chain(&new_logs)
                .enumerate()
                .map(|(i, log)| {
                    TreeInstruction::write(log.key.hashed_key_u256(), i as u64 + 1, log.value)
                })
                .collect();
            let expected_new_root_hash =
                ZkSyncTree::process_genesis_batch(&all_tree_instructions).root_hash;
            assert_ne!(expected_new_root_hash, snapshot_recovery.l1_batch_root_hash);
            assert_eq!(new_root_hash, expected_new_root_hash);
        }
    }

    stop_sender.send_replace(true);
    calculator_task.await.expect("calculator panicked").unwrap();
}

#[test_casing(3, [1, 2, 4])]
#[tokio::test]
async fn recovery_with_further_pruning(pruned_batches: u32) {
    const NEW_BATCH_COUNT: usize = 5;

    assert!(
        (pruned_batches as usize) < NEW_BATCH_COUNT,
        "at least 1 batch should remain in DB"
    );

    let pool = ConnectionPool::<Core>::test_pool().await;
    let snapshot_logs = gen_storage_logs(100..300, 1).pop().unwrap();
    let mut storage = pool.connection().await.unwrap();
    let mut db_transaction = storage.start_transaction().await.unwrap();
    let snapshot_recovery = prepare_recovery_snapshot(
        &mut db_transaction,
        L1BatchNumber(23),
        L2BlockNumber(42),
        &snapshot_logs,
    )
    .await;

    // Add some batches after recovery.
    let logs = gen_storage_logs(200..400, NEW_BATCH_COUNT);
    extend_db_state_from_l1_batch(
        &mut db_transaction,
        snapshot_recovery.l1_batch_number + 1,
        snapshot_recovery.l2_block_number + 1,
        logs,
    )
    .await;
    db_transaction.commit().await.unwrap();

    // Run the first tree instance to compute root hashes for all batches.
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) =
        setup_calculator(&temp_dir.path().join("first"), pool.clone(), true).await;
    let expected_root_hash = run_calculator(calculator).await;

    prune_storage(&pool, snapshot_recovery.l1_batch_number + pruned_batches).await;

    // Create a new tree instance. It should recover and process the remaining batches.
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(&temp_dir.path().join("new"), pool, true).await;
    assert_eq!(run_calculator(calculator).await, expected_root_hash);
}

#[tokio::test]
async fn detecting_root_hash_mismatch_after_pruning() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let snapshot_logs = gen_storage_logs(100..300, 1).pop().unwrap();
    let mut storage = pool.connection().await.unwrap();
    let mut db_transaction = storage.start_transaction().await.unwrap();
    let snapshot_recovery = prepare_recovery_snapshot(
        &mut db_transaction,
        L1BatchNumber(23),
        L2BlockNumber(42),
        &snapshot_logs,
    )
    .await;

    let logs = gen_storage_logs(200..400, 5);
    extend_db_state_from_l1_batch(
        &mut db_transaction,
        snapshot_recovery.l1_batch_number + 1,
        snapshot_recovery.l2_block_number + 1,
        logs,
    )
    .await;
    // Intentionally add an incorrect root has of the batch to be pruned.
    db_transaction
        .blocks_dal()
        .set_l1_batch_hash(snapshot_recovery.l1_batch_number + 1, H256::repeat_byte(42))
        .await
        .unwrap();
    db_transaction.commit().await.unwrap();

    prune_storage(&pool, snapshot_recovery.l1_batch_number + 1).await;

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let config = MetadataCalculatorRecoveryConfig::default();
    let (tree, _) = create_tree_recovery(temp_dir.path(), L1BatchNumber(1), &config).await;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count: 5,
        concurrency_limit: 1,
        events: Box::new(()),
    };
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");
    assert_eq!(init_params.expected_root_hash, Some(H256::repeat_byte(42)));

    let err = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap_err();
    let err = format!("{err:#}").to_lowercase();
    assert!(err.contains("root hash"), "{err}");

    // Because of an abrupt error, terminating a RocksDB instance needs to be handled explicitly.
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
}

#[derive(Debug)]
struct PruningEventListener {
    pool: ConnectionPool<Core>,
    pruned_l1_batch: L1BatchNumber,
}

#[async_trait]
impl HandleRecoveryEvent for PruningEventListener {
    async fn chunk_recovered(&self) {
        prune_storage(&self.pool, self.pruned_l1_batch).await;
    }
}

#[tokio::test]
async fn pruning_during_recovery_is_detected() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let logs = gen_storage_logs(200..400, 5);
    extend_db_state(&mut storage, logs).await;
    drop(storage);

    // Set root hashes for all L1 batches in Postgres.
    let (calculator, _) =
        setup_calculator(&temp_dir.path().join("first"), pool.clone(), true).await;
    run_calculator(calculator).await;
    prune_storage(&pool, L1BatchNumber(1)).await;

    let tree_path = temp_dir.path().join("recovery");
    let config = MetadataCalculatorRecoveryConfig::default();
    let (tree, _) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count: 5,
        concurrency_limit: 1,
        events: Box::new(PruningEventListener {
            pool: pool.clone(),
            pruned_l1_batch: L1BatchNumber(3),
        }),
    };
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");

    let err = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap_err();
    let err = format!("{err:#}").to_lowercase();
    assert!(err.contains("continuing recovery is impossible"), "{err}");

    // Because of an abrupt error, terminating a RocksDB instance needs to be handled explicitly.
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
}

async fn insert_large_genesis(storage: &mut Connection<'_, Core>) -> Vec<StorageLog> {
    let log_count = InitParameters::MIN_STORAGE_LOGS_FOR_GENESIS_RECOVERY * 2;
    let genesis_logs = gen_storage_logs(100..(log_count + 100), 1).pop().unwrap();
    let genesis_state = GenesisState {
        storage_logs: genesis_logs
            .iter()
            .map(|log| StorageLogRow {
                address: log.key.address().0,
                key: log.key.key().0,
                value: log.value.0,
            })
            .collect(),
        factory_deps: vec![], // Not necessary for tests
    };
    insert_genesis_batch_with_custom_state(storage, &GenesisParams::mock(), Some(genesis_state))
        .await
        .unwrap();
    genesis_logs
}

#[tokio::test]
async fn init_params_for_large_genesis_state() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let genesis_logs = insert_large_genesis(&mut pool.connection().await.unwrap()).await;

    let config = MetadataCalculatorRecoveryConfig::default();
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");
    assert_eq!(init_params.l1_batch, L1BatchNumber(0));
    assert_eq!(init_params.l2_block, L2BlockNumber(0));
    assert_eq!(init_params.log_count, genesis_logs.len() as u64);
    assert!(init_params.expected_root_hash.is_some());
}

#[tokio::test]
async fn entire_workflow_with_large_genesis_state() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut genesis_logs = insert_large_genesis(&mut pool.connection().await.unwrap()).await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let config = mock_config(temp_dir.path());
    let calculator = MetadataCalculator::new(config, None, pool).await.unwrap();
    let tree_reader = calculator.tree_reader();

    let (stop_sender, stop_receiver) = watch::channel(false);
    let calculator_task = tokio::spawn(calculator.run(stop_receiver));
    let tree_reader = tree_reader.wait().await.unwrap();
    let tree_info = tree_reader.info().await;
    assert_eq!(tree_info.leaf_count, genesis_logs.len() as u64);

    genesis_logs.sort_unstable_by_key(|log| log.key);
    let tree_instructions: Vec<_> = genesis_logs
        .into_iter()
        .enumerate()
        .map(|(i, log)| TreeInstruction::write(log.key.hashed_key_u256(), i as u64 + 1, log.value))
        .collect();
    let expected_genesis_root_hash =
        ZkSyncTree::process_genesis_batch(&tree_instructions).root_hash;
    assert_eq!(tree_info.root_hash, expected_genesis_root_hash);

    stop_sender.send_replace(true);
    calculator_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn fault_tolerance_for_large_genesis_state() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    insert_large_genesis(&mut pool.connection().await.unwrap()).await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let config = MetadataCalculatorRecoveryConfig::default();
    let (tree, _) = create_tree_recovery(temp_dir.path(), L1BatchNumber(0), &config).await;
    let (stop_sender, stop_receiver) = watch::channel(false);

    // Process 3 chunks and stop.
    let recovered_chunk_count = 3;
    let recovery_options = RecoveryOptions {
        chunk_count: 10,
        concurrency_limit: 1,
        events: Box::new(TestEventListener::new(recovered_chunk_count, stop_sender)),
    };
    let init_params = InitParameters::new(&pool, &config)
        .await
        .unwrap()
        .expect("no init params");
    let err = tree
        .recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap_err();
    assert_matches!(err, OrStopped::Stopped);

    // Restart recovery and process the remaining chunks
    let (tree, _) = create_tree_recovery(temp_dir.path(), L1BatchNumber(0), &config).await;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count: 10,
        concurrency_limit: 1,
        events: Box::new(
            TestEventListener::new(u64::MAX, stop_sender)
                .expect_recovered_chunks(recovered_chunk_count),
        ),
    };
    tree.recover(init_params, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap();
}

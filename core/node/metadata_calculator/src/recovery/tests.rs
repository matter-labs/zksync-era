//! Tests for metadata calculator snapshot recovery.

use std::{path::Path, sync::Mutex};

use assert_matches::assert_matches;
use tempfile::TempDir;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_config::configs::{
    chain::{OperationsManagerConfig, StateKeeperConfig},
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::CoreDal;
use zksync_health_check::{CheckHealth, HealthStatus, ReactiveHealthCheck};
use zksync_merkle_tree::{domain::ZkSyncTree, recovery::PersistenceThreadHandle, TreeInstruction};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_types::{L1BatchNumber, ProtocolVersionId, StorageLog};

use super::*;
use crate::{
    helpers::create_db,
    tests::{
        extend_db_state, extend_db_state_from_l1_batch, gen_storage_logs, mock_config,
        run_calculator, setup_calculator,
    },
    MetadataCalculator, MetadataCalculatorConfig,
};

#[test]
fn calculating_chunk_count() {
    let mut snapshot = SnapshotParameters {
        l2_block: L2BlockNumber(1),
        log_count: 160_000_000,
        expected_root_hash: H256::zero(),
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
    let snapshot_recovery = prepare_recovery_snapshot_with_genesis(pool.clone(), &temp_dir).await;
    let config = MetadataCalculatorRecoveryConfig::default();
    let snapshot = SnapshotParameters::new(&pool, &snapshot_recovery, &config)
        .await
        .unwrap();

    assert!(snapshot.log_count > 200);

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
            .recover(snapshot, recovery_options, &pool, &stop_receiver)
            .await
            .unwrap()
            .expect("Tree recovery unexpectedly aborted");

        assert_eq!(tree.root_hash(), snapshot_recovery.l1_batch_root_hash);
        let health = health_check.check_health().await;
        assert_matches!(health.status(), HealthStatus::Affected);
    }
}

async fn prepare_recovery_snapshot_with_genesis(
    pool: ConnectionPool<Core>,
    temp_dir: &TempDir,
) -> SnapshotRecoveryStatus {
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let mut logs = gen_storage_logs(100..300, 1).pop().unwrap();

    // Add all logs from the genesis L1 batch to `logs` so that they cover all state keys.
    let genesis_logs = storage
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();
    let genesis_logs = genesis_logs
        .into_iter()
        .map(|(key, value)| StorageLog::new_write_log(key, value));
    logs.extend(genesis_logs);
    extend_db_state(&mut storage, vec![logs]).await;
    drop(storage);

    // Ensure that metadata for L1 batch #1 is present in the DB.
    let (calculator, _) = setup_calculator(&temp_dir.path().join("init"), pool).await;
    let l1_batch_root_hash = run_calculator(calculator).await;

    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(1),
        l1_batch_timestamp: 1,
        l1_batch_root_hash,
        l2_block_number: L2BlockNumber(1),
        l2_block_timestamp: 1,
        l2_block_hash: H256::zero(), // not used
        protocol_version: ProtocolVersionId::latest(),
        storage_logs_chunks_processed: vec![],
    }
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

impl HandleRecoveryEvent for TestEventListener {
    fn recovery_started(&mut self, _chunk_count: u64, recovered_chunk_count: u64) {
        assert_eq!(recovered_chunk_count, self.expected_recovered_chunks);
    }

    fn chunk_recovered(&self) {
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
    let snapshot_recovery = prepare_recovery_snapshot_with_genesis(pool.clone(), &temp_dir).await;

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
    let snapshot = SnapshotParameters::new(&pool, &snapshot_recovery, &config)
        .await
        .unwrap();
    assert!(tree
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap()
        .is_none());

    // Emulate a restart and recover 2 more chunks (or 1 + emulated persistence crash).
    let (mut tree, handle) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    assert_ne!(tree.root_hash().await, snapshot_recovery.l1_batch_root_hash);
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
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await;
    if matches!(case, FaultToleranceCase::ParallelWithCrash) {
        let err = format!("{:#}", recovery_result.unwrap_err());
        assert!(err.contains("emulated persistence crash"), "{err}");
    } else {
        assert!(recovery_result.unwrap().is_none());
    }

    // Emulate another restart and recover remaining chunks.
    let (mut tree, _) = create_tree_recovery(&tree_path, L1BatchNumber(1), &config).await;
    assert_ne!(tree.root_hash().await, snapshot_recovery.l1_batch_root_hash);
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
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap()
        .expect("Tree recovery unexpectedly aborted");
    assert_eq!(tree.root_hash(), snapshot_recovery.l1_batch_root_hash);
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
    let merkle_tree_config = MerkleTreeConfig {
        path: temp_dir.path().to_str().unwrap().to_owned(),
        ..MerkleTreeConfig::default()
    };
    let calculator_config = MetadataCalculatorConfig::for_main_node(
        &merkle_tree_config,
        &OperationsManagerConfig { delay_interval: 50 },
        &StateKeeperConfig {
            protective_reads_persistence_enabled: true,
            ..Default::default()
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
                .map(|(i, log)| TreeInstruction::write(log.key, i as u64 + 1, log.value))
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

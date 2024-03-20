//! Tests for metadata calculator snapshot recovery.

use std::{path::PathBuf, time::Duration};

use assert_matches::assert_matches;
use tempfile::TempDir;
use test_casing::test_casing;
use tokio::sync::mpsc;
use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::CoreDal;
use zksync_health_check::{CheckHealth, HealthStatus, ReactiveHealthCheck};
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_types::{L1BatchNumber, ProtocolVersionId, StorageLog};

use super::*;
use crate::{
    genesis::{insert_genesis_batch, GenesisParams},
    metadata_calculator::{
        helpers::create_db,
        tests::{
            extend_db_state, extend_db_state_from_l1_batch, gen_storage_logs, run_calculator,
            setup_calculator,
        },
        MetadataCalculator, MetadataCalculatorConfig,
    },
    utils::testonly::prepare_recovery_snapshot,
};

#[test]
fn calculating_chunk_count() {
    let mut snapshot = SnapshotParameters {
        miniblock: MiniblockNumber(1),
        log_count: 160_000_000,
        expected_root_hash: H256::zero(),
    };
    assert_eq!(snapshot.chunk_count(), 800);

    snapshot.log_count += 1;
    assert_eq!(snapshot.chunk_count(), 801);

    snapshot.log_count = 100;
    assert_eq!(snapshot.chunk_count(), 1);
}

async fn create_tree_recovery(path: PathBuf, l1_batch: L1BatchNumber) -> AsyncTreeRecovery {
    let db = create_db(
        path,
        0,
        16 << 20,       // 16 MiB,
        Duration::ZERO, // writes should never be stalled in tests
        500,
    )
    .await
    .unwrap();
    AsyncTreeRecovery::new(db, l1_batch.0.into(), MerkleTreeMode::Full)
}

#[tokio::test]
async fn basic_recovery_workflow() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let snapshot_recovery = prepare_recovery_snapshot_with_genesis(&pool, &temp_dir).await;
    let snapshot = SnapshotParameters::new(&pool, &snapshot_recovery)
        .await
        .unwrap();

    assert!(snapshot.log_count > 200);

    let (_stop_sender, stop_receiver) = watch::channel(false);
    for chunk_count in [1, 4, 9, 16, 60, 256] {
        println!("Recovering tree with {chunk_count} chunks");

        let tree_path = temp_dir.path().join(format!("recovery-{chunk_count}"));
        let tree = create_tree_recovery(tree_path, L1BatchNumber(1)).await;
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
        assert_matches!(health.status(), HealthStatus::Ready);
    }
}

async fn prepare_recovery_snapshot_with_genesis(
    pool: &ConnectionPool<Core>,
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
    let l1_batch_root_hash = run_calculator(calculator, pool.clone()).await;

    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(1),
        l1_batch_timestamp: 1,
        l1_batch_root_hash,
        miniblock_number: MiniblockNumber(1),
        miniblock_timestamp: 1,
        miniblock_hash: H256::zero(), // not used
        protocol_version: ProtocolVersionId::latest(),
        storage_logs_chunks_processed: vec![],
    }
}

#[derive(Debug)]
struct TestEventListener {
    expected_recovered_chunks: u64,
    stop_threshold: u64,
    processed_chunk_count: AtomicU64,
    stop_sender: watch::Sender<bool>,
}

impl TestEventListener {
    fn new(stop_threshold: u64, stop_sender: watch::Sender<bool>) -> Self {
        Self {
            expected_recovered_chunks: 0,
            stop_threshold,
            processed_chunk_count: AtomicU64::new(0),
            stop_sender,
        }
    }

    fn expect_recovered_chunks(mut self, count: u64) -> Self {
        self.expected_recovered_chunks = count;
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
    }
}

#[test_casing(3, [5, 7, 8])]
#[tokio::test]
async fn recovery_fault_tolerance(chunk_count: u64) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let snapshot_recovery = prepare_recovery_snapshot_with_genesis(&pool, &temp_dir).await;

    let tree_path = temp_dir.path().join("recovery");
    let tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(TestEventListener::new(1, stop_sender)),
    };
    let snapshot = SnapshotParameters::new(&pool, &snapshot_recovery)
        .await
        .unwrap();
    assert!(tree
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap()
        .is_none());

    // Emulate a restart and recover 2 more chunks.
    let mut tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
    assert_ne!(tree.root_hash().await, snapshot_recovery.l1_batch_root_hash);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(TestEventListener::new(2, stop_sender).expect_recovered_chunks(1)),
    };
    assert!(tree
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap()
        .is_none());

    // Emulate another restart and recover remaining chunks.
    let mut tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
    assert_ne!(tree.root_hash().await, snapshot_recovery.l1_batch_root_hash);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let recovery_options = RecoveryOptions {
        chunk_count,
        concurrency_limit: 1,
        events: Box::new(TestEventListener::new(u64::MAX, stop_sender).expect_recovered_chunks(3)),
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
        MiniblockNumber(42),
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
    );
    let mut calculator = MetadataCalculator::new(calculator_config, None)
        .await
        .unwrap();
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let tree_reader = calculator.tree_reader();
    let calculator_task = tokio::spawn(calculator.run(pool.clone(), stop_receiver));

    match case {
        // Wait until the tree is fully initialized and stop the calculator.
        RecoveryWorkflowCase::Stop => {
            let tree_info = tree_reader.wait().await.info().await;
            assert_eq!(tree_info.root_hash, snapshot_recovery.l1_batch_root_hash);
            assert_eq!(tree_info.leaf_count, 200);
            assert_eq!(
                tree_info.next_l1_batch_number,
                snapshot_recovery.l1_batch_number + 1
            );
        }

        // Emulate state keeper adding a new L1 batch to Postgres.
        RecoveryWorkflowCase::CreateBatch => {
            tree_reader.wait().await;

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

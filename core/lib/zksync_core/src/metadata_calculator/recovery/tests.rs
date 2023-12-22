//! Tests for metadata calculator snapshot recovery.

use std::{path::PathBuf, time::Duration};

use assert_matches::assert_matches;
use tempfile::TempDir;
use test_casing::test_casing;
use zksync_config::configs::database::MerkleTreeMode;
use zksync_health_check::{CheckHealth, ReactiveHealthCheck};
use zksync_types::{L1BatchNumber, L2ChainId, StorageLog};
use zksync_utils::h256_to_u256;

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    metadata_calculator::{
        helpers::create_db,
        tests::{extend_db_state, gen_storage_logs, run_calculator, setup_calculator},
    },
};

#[test]
fn calculating_hashed_key_ranges_with_single_chunk() {
    let mut ranges = AsyncTreeRecovery::hashed_key_ranges(1);
    let full_range = ranges.next().unwrap();
    assert_eq!(full_range, H256::zero()..=H256([0xff; 32]));
}

#[test]
fn calculating_hashed_key_ranges_for_256_chunks() {
    let ranges = AsyncTreeRecovery::hashed_key_ranges(256);
    let mut start = H256::zero();
    let mut end = H256([0xff; 32]);

    for (i, range) in ranges.enumerate() {
        let i = u8::try_from(i).unwrap();
        start.0[0] = i;
        end.0[0] = i;
        assert_eq!(range, start..=end);
    }
}

#[test_casing(5, [3, 7, 23, 100, 255])]
fn calculating_hashed_key_ranges_for_arbitrary_chunks(chunk_count: usize) {
    let ranges: Vec<_> = AsyncTreeRecovery::hashed_key_ranges(chunk_count).collect();
    assert_eq!(ranges.len(), chunk_count);

    for window in ranges.windows(2) {
        let [prev_range, range] = window else {
            unreachable!();
        };
        assert_eq!(
            h256_to_u256(*range.start()),
            h256_to_u256(*prev_range.end()) + 1
        );
    }
    assert_eq!(*ranges.first().unwrap().start(), H256::zero());
    assert_eq!(*ranges.last().unwrap().end(), H256([0xff; 32]));
}

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
    .await;
    AsyncTreeRecovery::new(db, l1_batch.0.into(), MerkleTreeMode::Full)
}

#[tokio::test]
async fn basic_recovery_workflow() {
    let pool = ConnectionPool::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let snapshot_recovery = prepare_recovery_snapshot(&pool, &temp_dir).await;
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

async fn prepare_recovery_snapshot(
    pool: &ConnectionPool,
    temp_dir: &TempDir,
) -> SnapshotRecoveryStatus {
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::from(270), &GenesisParams::mock())
        .await
        .unwrap();
    let mut logs = gen_storage_logs(100..300, 1).pop().unwrap();

    // Add all logs from the genesis L1 batch to `logs` so that they cover all state keys.
    let genesis_logs = storage
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(L1BatchNumber(0))
        .await;
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
        l1_batch_root_hash,
        miniblock_number: MiniblockNumber(1),
        miniblock_root_hash: H256::zero(), // not used
        last_finished_chunk_id: Some(0),
        total_chunk_count: 1,
    }
}

#[derive(Debug)]
struct TestEventListener {
    expected_recovered_chunks: usize,
    stop_threshold: usize,
    processed_chunk_count: AtomicUsize,
    stop_sender: watch::Sender<bool>,
}

impl TestEventListener {
    fn new(stop_threshold: usize, stop_sender: watch::Sender<bool>) -> Self {
        Self {
            expected_recovered_chunks: 0,
            stop_threshold,
            processed_chunk_count: AtomicUsize::new(0),
            stop_sender,
        }
    }

    fn expect_recovered_chunks(mut self, count: usize) -> Self {
        self.expected_recovered_chunks = count;
        self
    }
}

#[async_trait]
impl HandleRecoveryEvent for TestEventListener {
    fn recovery_started(&mut self, _chunk_count: usize, recovered_chunk_count: usize) {
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
async fn recovery_fault_tolerance(chunk_count: usize) {
    let pool = ConnectionPool::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let snapshot_recovery = prepare_recovery_snapshot(&pool, &temp_dir).await;

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
        events: Box::new(
            TestEventListener::new(usize::MAX, stop_sender).expect_recovered_chunks(3),
        ),
    };
    let tree = tree
        .recover(snapshot, recovery_options, &pool, &stop_receiver)
        .await
        .unwrap()
        .expect("Tree recovery unexpectedly aborted");
    assert_eq!(tree.root_hash(), snapshot_recovery.l1_batch_root_hash);
}

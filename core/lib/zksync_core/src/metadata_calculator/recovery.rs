//! High-level recovery logic for the Merkle tree.

use anyhow::Context as _;
use async_trait::async_trait;
use futures::future;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};

use std::{
    fmt, ops,
    sync::atomic::{AtomicUsize, Ordering},
};

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_merkle_tree::TreeEntry;
use zksync_types::{L1BatchNumber, MiniblockNumber, H256, U256};
use zksync_utils::u256_to_h256;

use super::{
    helpers::{AsyncTree, AsyncTreeRecovery, GenericAsyncTree},
    metrics::{ChunkRecoveryStage, RecoveryStage, RECOVERY_METRICS},
};

#[async_trait]
trait HandleRecoveryEvent: fmt::Debug + Send + Sync {
    fn recovery_started(&mut self, _chunk_count: usize, _recovered_chunk_count: usize) {
        // Default implementation does nothing
    }

    async fn chunk_started(&self) {
        // Default implementation does nothing
    }

    async fn chunk_recovered(&self) {
        // Default implementation does nothing
    }
}

/// Information about a Merkle tree during its snapshot recovery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct RecoveryMerkleTreeInfo {
    mode: &'static str, // always set to "recovery" to distinguish from `MerkleTreeInfo`
    chunk_count: usize,
    recovered_chunk_count: usize,
}

/// [`HealthUpdater`]-based [`HandleRecoveryEvent`] implementation.
#[derive(Debug)]
struct RecoveryHealthUpdater<'a> {
    inner: &'a HealthUpdater,
    chunk_count: usize,
    recovered_chunk_count: AtomicUsize,
}

impl<'a> RecoveryHealthUpdater<'a> {
    fn new(inner: &'a HealthUpdater) -> Self {
        Self {
            inner,
            chunk_count: 0,
            recovered_chunk_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl HandleRecoveryEvent for RecoveryHealthUpdater<'_> {
    fn recovery_started(&mut self, chunk_count: usize, recovered_chunk_count: usize) {
        self.chunk_count = chunk_count;
        *self.recovered_chunk_count.get_mut() = recovered_chunk_count;
        RECOVERY_METRICS
            .recovered_chunk_count
            .set(recovered_chunk_count);
    }

    async fn chunk_recovered(&self) {
        let recovered_chunk_count = self.recovered_chunk_count.fetch_add(1, Ordering::SeqCst) + 1;
        RECOVERY_METRICS
            .recovered_chunk_count
            .set(recovered_chunk_count);
        let health = Health::from(HealthStatus::Ready).with_details(RecoveryMerkleTreeInfo {
            mode: "recovery",
            chunk_count: self.chunk_count,
            recovered_chunk_count,
        });
        self.inner.update(health);
    }
}

#[derive(Debug)]
struct RecoveryOptions<'a> {
    chunk_count: usize,
    events: Box<dyn HandleRecoveryEvent + 'a>,
}

impl GenericAsyncTree {
    /// Ensures that the tree is ready for the normal operation, recovering it from a Postgres snapshot
    /// if necessary.
    pub async fn ensure_ready(
        self,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
        health_updater: &HealthUpdater,
    ) -> anyhow::Result<Option<AsyncTree>> {
        let (tree, l1_batch) = match self {
            Self::Ready(tree) => return Ok(Some(tree)),
            Self::Recovering(tree) => {
                let l1_batch = snapshot_l1_batch(pool).await?.context(
                    "Merkle tree is recovering, but Postgres doesn't contain snapshot L1 batch",
                )?;
                let recovered_version = tree.recovered_version();
                anyhow::ensure!(
                    u64::from(l1_batch.0) == recovered_version,
                    "Snapshot L1 batch in Postgres ({l1_batch}) differs from the recovered Merkle tree version \
                     ({recovered_version})"
                );
                tracing::info!("Resuming tree recovery with snapshot L1 batch #{l1_batch}");
                (tree, l1_batch)
            }
            Self::Empty { db, mode } => {
                if let Some(l1_batch) = snapshot_l1_batch(pool).await? {
                    tracing::info!(
                        "Starting Merkle tree recovery with snapshot L1 batch #{l1_batch}"
                    );
                    let tree = AsyncTreeRecovery::new(db, l1_batch.0.into(), mode);
                    (tree, l1_batch)
                } else {
                    // Start the tree from scratch. The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    return Ok(Some(AsyncTree::new(db, mode)));
                }
            }
        };

        // FIXME: make choice of ranges more intelligent. NB: ranges must be the same for the entire recovery!
        let recovery_options = RecoveryOptions {
            chunk_count: 256,
            events: Box::new(RecoveryHealthUpdater::new(health_updater)),
        };
        tree.recover(l1_batch, recovery_options, pool, stop_receiver)
            .await
    }
}

impl AsyncTreeRecovery {
    async fn recover(
        mut self,
        l1_batch: L1BatchNumber,
        mut options: RecoveryOptions<'_>,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<AsyncTree>> {
        let mut storage = pool.access_storage().await?;
        let (_, snapshot_miniblock) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch)
            .await
            .with_context(|| format!("Failed getting miniblock range for L1 batch #{l1_batch}"))?
            .with_context(|| format!("L1 batch #{l1_batch} doesn't have miniblocks"))?;
        let expected_root_hash = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch)
            .await
            .with_context(|| format!("Failed getting metadata for L1 batch #{l1_batch}"))?
            .with_context(|| format!("L1 batch #{l1_batch} has no metadata"))?
            .metadata
            .root_hash;

        let chunk_count = options.chunk_count;
        let chunks: Vec<_> = Self::hashed_key_ranges(chunk_count).collect();
        tracing::info!(
            "Recovering Merkle tree from Postgres snapshot in {chunk_count} concurrent chunks"
        );
        let remaining_chunks = self
            .filter_chunks(&mut storage, snapshot_miniblock, &chunks)
            .await?;
        drop(storage);
        options
            .events
            .recovery_started(chunk_count, chunk_count - remaining_chunks.len());
        tracing::info!(
            "Filtered recovered key chunks; {} / {chunk_count} chunks remaining",
            remaining_chunks.len()
        );

        let tree = Mutex::new(self);
        let chunk_tasks = remaining_chunks.into_iter().map(|chunk| async {
            options.events.chunk_started().await;
            Self::recover_key_chunk(&tree, snapshot_miniblock, chunk, pool, stop_receiver).await?;
            options.events.chunk_recovered().await;
            anyhow::Ok(())
        });
        future::try_join_all(chunk_tasks).await?;

        if *stop_receiver.borrow() {
            return Ok(None);
        }

        let finalize_latency = RECOVERY_METRICS.latency[&RecoveryStage::Finalize].start();
        let mut tree = tree.into_inner();
        let actual_root_hash = tree.root_hash().await;
        anyhow::ensure!(
            actual_root_hash == expected_root_hash,
            "Root hash of recovered tree {actual_root_hash:?} differs from expected root hash {expected_root_hash:?}"
        );
        let tree = tree.finalize().await;
        let finalize_latency = finalize_latency.observe();
        tracing::info!(
            "Finished tree recovery in {finalize_latency:?}; resuming normal tree operation"
        );
        Ok(Some(tree))
    }

    fn hashed_key_ranges(count: usize) -> impl Iterator<Item = ops::RangeInclusive<H256>> {
        assert!(count > 0);
        let mut stride = U256::MAX / count;
        let stride_minus_one = if stride < U256::MAX {
            stride += U256::one();
            stride - 1
        } else {
            stride // `stride` is really 1 << 256 == U256::MAX + 1
        };

        (0..count).map(move |i| {
            let start = stride * i;
            let (mut end, is_overflow) = stride_minus_one.overflowing_add(start);
            if is_overflow {
                end = U256::MAX;
            }
            u256_to_h256(start)..=u256_to_h256(end)
        })
    }

    /// Filters out `key_chunks` for which recovery was successfully performed.
    async fn filter_chunks(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        snapshot_miniblock: MiniblockNumber,
        key_chunks: &[ops::RangeInclusive<H256>],
    ) -> anyhow::Result<Vec<ops::RangeInclusive<H256>>> {
        let chunk_starts_latency =
            RECOVERY_METRICS.latency[&RecoveryStage::LoadChunkStarts].start();
        let chunk_starts = storage
            .storage_logs_dal()
            .get_chunk_starts_for_miniblock(snapshot_miniblock, key_chunks)
            .await
            .context("Failed getting chunk starts")?;
        let chunk_starts_latency = chunk_starts_latency.observe();
        tracing::debug!(
            "Loaded start entries for {} chunks in {chunk_starts_latency:?}",
            key_chunks.len()
        );

        let existing_starts = chunk_starts
            .iter()
            .enumerate()
            .filter_map(|(i, &start)| Some((i, start?)));
        let start_keys = existing_starts
            .clone()
            .map(|(_, start_entry)| start_entry.hashed_key)
            .collect();
        let tree_entries = self.entries(start_keys).await;

        let mut output = vec![];
        for (tree_entry, (i, db_entry)) in tree_entries.into_iter().zip(existing_starts) {
            if tree_entry.is_empty() {
                output.push(key_chunks[i].clone());
                continue;
            }
            anyhow::ensure!(
                tree_entry.value == db_entry.value && tree_entry.leaf_index == db_entry.index,
                "Mismatch between entry for key {:0>64x} in Postgres snapshot for miniblock #{snapshot_miniblock} \
                 ({db_entry:?}) and tree ({tree_entry:?}); the recovery procedure may be corrupted",
                db_entry.hashed_key
            );
        }
        Ok(output)
    }

    async fn recover_key_chunk(
        tree: &Mutex<AsyncTreeRecovery>,
        snapshot_miniblock: MiniblockNumber,
        key_chunk: ops::RangeInclusive<H256>,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let acquire_connection_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::AcquireConnection].start();
        let mut storage = pool.access_storage().await?;
        acquire_connection_latency.observe();

        if *stop_receiver.borrow() {
            return Ok(());
        }

        let entries_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LoadEntries].start();
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone(), None)
            .await
            .with_context(|| {
                format!("Failed getting entries for chunk {key_chunk:?} in snapshot for miniblock #{snapshot_miniblock}")
            })?;
        drop(storage);
        let entries_latency = entries_latency.observe();
        tracing::debug!(
            "Loaded {} entries for chunk {key_chunk:?} in {entries_latency:?}",
            all_entries.len()
        );

        if *stop_receiver.borrow() {
            return Ok(());
        }

        // Sanity check: all entry keys must be distinct. Otherwise, we may end up writing non-final values
        // to the tree, since we don't enforce any ordering on entries besides by the hashed key.
        for window in all_entries.windows(2) {
            let [prev_entry, next_entry] = window else {
                unreachable!();
            };
            anyhow::ensure!(
                prev_entry.hashed_key != next_entry.hashed_key,
                "node snapshot in Postgres is corrupted: entries {prev_entry:?} and {next_entry:?} \
                 have same hashed_key"
            );
        }

        let all_entries = all_entries
            .into_iter()
            .map(|entry| TreeEntry {
                key: entry.hashed_key,
                value: entry.value,
                leaf_index: entry.index,
            })
            .collect();
        let lock_tree_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LockTree].start();
        let mut tree = tree.lock().await;
        lock_tree_latency.observe();

        if *stop_receiver.borrow() {
            return Ok(());
        }

        let extend_tree_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::ExtendTree].start();
        tree.extend(all_entries).await;
        let extend_tree_latency = extend_tree_latency.observe();
        tracing::debug!(
            "Extended Merkle tree with entries for chunk {key_chunk:?} in {extend_tree_latency:?}"
        );
        Ok(())
    }
}

async fn snapshot_l1_batch(_pool: &ConnectionPool) -> anyhow::Result<Option<L1BatchNumber>> {
    Ok(None) // FIXME: implement real logic
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tempfile::TempDir;
    use test_casing::test_casing;
    use tokio::sync::OwnedMutexGuard;

    use std::{path::PathBuf, sync::Arc, time::Duration};

    use zksync_config::configs::database::MerkleTreeMode;
    use zksync_health_check::{CheckHealth, ReactiveHealthCheck};
    use zksync_types::{L2ChainId, StorageLog};
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
        let root_hash = prepare_recovery_snapshot(&pool, &temp_dir).await;

        let (_stop_sender, stop_receiver) = watch::channel(false);
        for chunk_count in [1, 4, 9, 16, 60, 256] {
            println!("Recovering tree with {chunk_count} chunks");

            let tree_path = temp_dir.path().join(format!("recovery-{chunk_count}"));
            let tree = create_tree_recovery(tree_path, L1BatchNumber(1)).await;
            let (health_check, health_updater) = ReactiveHealthCheck::new("tree");
            let recovery_options = RecoveryOptions {
                chunk_count,
                events: Box::new(RecoveryHealthUpdater::new(&health_updater)),
            };
            let tree = tree
                .recover(L1BatchNumber(1), recovery_options, &pool, &stop_receiver)
                .await
                .unwrap()
                .expect("Tree recovery unexpectedly aborted");

            assert_eq!(tree.root_hash(), root_hash);
            let health = health_check.check_health().await;
            assert_matches!(health.status(), HealthStatus::Ready);
        }
    }

    async fn prepare_recovery_snapshot(pool: &ConnectionPool, temp_dir: &TempDir) -> H256 {
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
        let prover_pool = ConnectionPool::test_pool().await;
        run_calculator(calculator, pool.clone(), prover_pool).await
    }

    #[derive(Debug)]
    struct TestEventListener {
        expected_recovered_chunks: usize,
        stop_threshold: usize,
        processed_chunk_count: AtomicUsize,
        stop_sender: watch::Sender<bool>,
        // We want to precisely control chunk recovery, so we linearize it via `chunk_started()`
        // `chunk_recovered()` hooks using a mutex. Using a DB pool with a single connection would
        // be imprecise, since a connection is dropped before the chunk recovery is fully finished.
        mutex: Arc<Mutex<()>>,
        acquired_guard: Mutex<Option<OwnedMutexGuard<()>>>,
    }

    impl TestEventListener {
        fn new(stop_threshold: usize, stop_sender: watch::Sender<bool>) -> Self {
            Self {
                expected_recovered_chunks: 0,
                stop_threshold,
                processed_chunk_count: AtomicUsize::new(0),
                stop_sender,
                mutex: Arc::default(),
                acquired_guard: Mutex::default(),
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

        async fn chunk_started(&self) {
            let guard = Arc::clone(&self.mutex).lock_owned().await;
            *self.acquired_guard.lock().await = Some(guard);
        }

        async fn chunk_recovered(&self) {
            let processed_chunk_count =
                self.processed_chunk_count.fetch_add(1, Ordering::SeqCst) + 1;
            if processed_chunk_count >= self.stop_threshold {
                self.stop_sender.send_replace(true);
            }
            *self.acquired_guard.lock().await = None;
        }
    }

    #[test_casing(3, [5, 7, 8])]
    #[tokio::test]
    async fn recovery_fault_tolerance(chunk_count: usize) {
        let pool = ConnectionPool::test_pool().await;
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let root_hash = prepare_recovery_snapshot(&pool, &temp_dir).await;

        let tree_path = temp_dir.path().join("recovery");
        let tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let recovery_options = RecoveryOptions {
            chunk_count,
            events: Box::new(TestEventListener::new(1, stop_sender)),
        };
        assert!(tree
            .recover(L1BatchNumber(1), recovery_options, &pool, &stop_receiver)
            .await
            .unwrap()
            .is_none());

        // Emulate a restart and recover 2 more chunks.
        let mut tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
        assert_ne!(tree.root_hash().await, root_hash);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let recovery_options = RecoveryOptions {
            chunk_count,
            events: Box::new(TestEventListener::new(2, stop_sender).expect_recovered_chunks(1)),
        };
        assert!(tree
            .recover(L1BatchNumber(1), recovery_options, &pool, &stop_receiver)
            .await
            .unwrap()
            .is_none());

        // Emulate another restart and recover remaining chunks.
        let mut tree = create_tree_recovery(tree_path.clone(), L1BatchNumber(1)).await;
        assert_ne!(tree.root_hash().await, root_hash);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let recovery_options = RecoveryOptions {
            chunk_count,
            events: Box::new(
                TestEventListener::new(usize::MAX, stop_sender).expect_recovered_chunks(3),
            ),
        };
        let tree = tree
            .recover(L1BatchNumber(1), recovery_options, &pool, &stop_receiver)
            .await
            .unwrap()
            .expect("Tree recovery unexpectedly aborted");
        assert_eq!(tree.root_hash(), root_hash);
    }
}

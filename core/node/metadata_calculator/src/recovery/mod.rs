//! High-level recovery logic for the Merkle tree.
//!
//! # Overview
//!
//! Tree recovery works by checking Postgres and Merkle tree state on Metadata calculator initialization.
//! Depending on these states, we can have one of the following situations:
//!
//! - Tree is recovering.
//! - Tree is empty and should be recovered (i.e., there's a snapshot in Postgres).
//! - Tree is empty and should be built from scratch.
//! - Tree is ready for normal operation (i.e., it's not empty and is not recovering).
//!
//! If recovery is necessary, it starts / resumes by loading the Postgres snapshot in chunks
//! and feeding each chunk to the tree. Chunks are loaded concurrently since this is the most
//! I/O-heavy operation; the concurrency is naturally limited by the number of connections to
//! Postgres in the supplied connection pool, but we explicitly use a [`Semaphore`] to control it
//! in order to not run into DB timeout errors. Before starting recovery in chunks, we filter out
//! chunks that have already been recovered by checking if the first key in a chunk is present
//! in the tree. (Note that for this to work, chunks **must** always be defined in the same way.)
//!
//! The recovery logic is fault-tolerant and supports graceful shutdown. If recovery is interrupted,
//! recovery of the remaining chunks will continue when Metadata calculator is restarted.
//!
//! Recovery performs basic sanity checks to ensure that the tree won't end up containing garbage data.
//! E.g., it's checked that the tree always recovers from the same snapshot; that the tree root hash
//! after recovery matches one in the Postgres snapshot etc.

use std::{
    fmt, ops,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use anyhow::Context as _;
use futures::future;
use tokio::sync::{watch, Mutex, Semaphore};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_health_check::HealthUpdater;
use zksync_merkle_tree::TreeEntry;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_types::{
    snapshots::{uniform_hashed_keys_chunk, SnapshotRecoveryStatus},
    L2BlockNumber, H256,
};

use super::{
    helpers::{AsyncTree, AsyncTreeRecovery, GenericAsyncTree, MerkleTreeHealth},
    metrics::{ChunkRecoveryStage, RecoveryStage, RECOVERY_METRICS},
    MetadataCalculatorRecoveryConfig,
};

#[cfg(test)]
mod tests;

/// Handler of recovery life cycle events. This functionality is encapsulated in a trait to be able
/// to control recovery behavior in tests.
trait HandleRecoveryEvent: fmt::Debug + Send + Sync {
    fn recovery_started(&mut self, _chunk_count: u64, _recovered_chunk_count: u64) {
        // Default implementation does nothing
    }

    fn chunk_recovered(&self) {
        // Default implementation does nothing
    }
}

/// [`HealthUpdater`]-based [`HandleRecoveryEvent`] implementation.
#[derive(Debug)]
struct RecoveryHealthUpdater<'a> {
    inner: &'a HealthUpdater,
    chunk_count: u64,
    recovered_chunk_count: AtomicU64,
}

impl<'a> RecoveryHealthUpdater<'a> {
    fn new(inner: &'a HealthUpdater) -> Self {
        Self {
            inner,
            chunk_count: 0,
            recovered_chunk_count: AtomicU64::new(0),
        }
    }
}

impl HandleRecoveryEvent for RecoveryHealthUpdater<'_> {
    fn recovery_started(&mut self, chunk_count: u64, recovered_chunk_count: u64) {
        self.chunk_count = chunk_count;
        *self.recovered_chunk_count.get_mut() = recovered_chunk_count;
        RECOVERY_METRICS
            .recovered_chunk_count
            .set(recovered_chunk_count);
    }

    fn chunk_recovered(&self) {
        let recovered_chunk_count = self.recovered_chunk_count.fetch_add(1, Ordering::SeqCst) + 1;
        let chunks_left = self.chunk_count.saturating_sub(recovered_chunk_count);
        tracing::info!(
            "Recovered {recovered_chunk_count}/{} Merkle tree chunks, there are {chunks_left} left to process",
            self.chunk_count
        );
        RECOVERY_METRICS
            .recovered_chunk_count
            .set(recovered_chunk_count);
        let health = MerkleTreeHealth::Recovery {
            chunk_count: self.chunk_count,
            recovered_chunk_count,
        };
        self.inner.update(health.into());
    }
}

#[derive(Debug, Clone, Copy)]
struct SnapshotParameters {
    l2_block: L2BlockNumber,
    expected_root_hash: H256,
    log_count: u64,
    desired_chunk_size: u64,
}

impl SnapshotParameters {
    async fn new(
        pool: &ConnectionPool<Core>,
        recovery: &SnapshotRecoveryStatus,
        config: &MetadataCalculatorRecoveryConfig,
    ) -> anyhow::Result<Self> {
        let l2_block = recovery.l2_block_number;
        let expected_root_hash = recovery.l1_batch_root_hash;

        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        let log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(l2_block)
            .await?;

        Ok(Self {
            l2_block,
            expected_root_hash,
            log_count,
            desired_chunk_size: config.desired_chunk_size,
        })
    }

    fn chunk_count(&self) -> u64 {
        self.log_count.div_ceil(self.desired_chunk_size)
    }
}

/// Options for tree recovery.
#[derive(Debug)]
struct RecoveryOptions<'a> {
    chunk_count: u64,
    concurrency_limit: usize,
    events: Box<dyn HandleRecoveryEvent + 'a>,
}

impl GenericAsyncTree {
    /// Ensures that the tree is ready for the normal operation, recovering it from a Postgres snapshot
    /// if necessary.
    ///
    /// `recovery_pool` is taken by value to free up its connection after recovery (provided that it's not shared
    /// with other components).
    pub async fn ensure_ready(
        self,
        config: &MetadataCalculatorRecoveryConfig,
        main_pool: &ConnectionPool<Core>,
        recovery_pool: ConnectionPool<Core>,
        health_updater: &HealthUpdater,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<AsyncTree>> {
        let started_at = Instant::now();
        let (tree, snapshot_recovery) = match self {
            Self::Ready(tree) => return Ok(Some(tree)),
            Self::Recovering(tree) => {
                let snapshot_recovery = get_snapshot_recovery(main_pool).await?.context(
                    "Merkle tree is recovering, but Postgres doesn't contain snapshot recovery information",
                )?;
                let recovered_version = tree.recovered_version();
                anyhow::ensure!(
                    u64::from(snapshot_recovery.l1_batch_number.0) == recovered_version,
                    "Snapshot L1 batch in Postgres ({snapshot_recovery:?}) differs from the recovered Merkle tree version \
                     ({recovered_version})"
                );
                tracing::info!("Resuming tree recovery with status: {snapshot_recovery:?}");
                (tree, snapshot_recovery)
            }
            Self::Empty { db, mode } => {
                if let Some(snapshot_recovery) = get_snapshot_recovery(main_pool).await? {
                    tracing::info!(
                        "Starting Merkle tree recovery with status {snapshot_recovery:?}"
                    );
                    let l1_batch = snapshot_recovery.l1_batch_number;
                    let tree = AsyncTreeRecovery::new(db, l1_batch.0.into(), mode, config)?;
                    (tree, snapshot_recovery)
                } else {
                    // Start the tree from scratch. The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    return Ok(Some(AsyncTree::new(db, mode)?));
                }
            }
        };

        let snapshot = SnapshotParameters::new(main_pool, &snapshot_recovery, config).await?;
        tracing::debug!(
            "Obtained snapshot parameters: {snapshot:?} based on recovery configuration {config:?}"
        );
        let recovery_options = RecoveryOptions {
            chunk_count: snapshot.chunk_count(),
            concurrency_limit: recovery_pool.max_size() as usize,
            events: Box::new(RecoveryHealthUpdater::new(health_updater)),
        };
        let tree = tree
            .recover(snapshot, recovery_options, &recovery_pool, stop_receiver)
            .await?;
        if tree.is_some() {
            // Only report latency if recovery wasn't canceled
            let elapsed = started_at.elapsed();
            APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::Tree].set(elapsed);
            tracing::info!("Recovered Merkle tree from snapshot in {elapsed:?}");
        }
        Ok(tree)
    }
}

impl AsyncTreeRecovery {
    async fn recover(
        mut self,
        snapshot: SnapshotParameters,
        mut options: RecoveryOptions<'_>,
        pool: &ConnectionPool<Core>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<AsyncTree>> {
        self.ensure_desired_chunk_size(snapshot.desired_chunk_size)
            .await?;

        let start_time = Instant::now();
        let chunk_count = options.chunk_count;
        let chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        tracing::info!(
            "Recovering Merkle tree from Postgres snapshot in {chunk_count} chunks with max concurrency {}",
            options.concurrency_limit
        );

        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        let remaining_chunks = self
            .filter_chunks(&mut storage, snapshot.l2_block, &chunks)
            .await?;
        drop(storage);
        options
            .events
            .recovery_started(chunk_count, chunk_count - remaining_chunks.len() as u64);
        tracing::info!(
            "Filtered recovered key chunks; {} / {chunk_count} chunks remaining",
            remaining_chunks.len()
        );

        let tree = Mutex::new(self);
        let semaphore = Semaphore::new(options.concurrency_limit);
        let chunk_tasks = remaining_chunks.into_iter().map(|chunk| async {
            let _permit = semaphore
                .acquire()
                .await
                .context("semaphore is never closed")?;
            if Self::recover_key_chunk(&tree, snapshot.l2_block, chunk, pool, stop_receiver).await?
            {
                options.events.chunk_recovered();
            }
            anyhow::Ok(())
        });
        future::try_join_all(chunk_tasks).await?;

        let mut tree = tree.into_inner();
        if *stop_receiver.borrow() {
            // Waiting for persistence is mostly useful for tests. Normally, the tree database won't be used in the same process
            // after a stop signal is received, so there's no risk of data races with the background persistence thread.
            tree.wait_for_persistence().await?;
            return Ok(None);
        }

        let finalize_latency = RECOVERY_METRICS.latency[&RecoveryStage::Finalize].start();
        let actual_root_hash = tree.root_hash().await;
        anyhow::ensure!(
            actual_root_hash == snapshot.expected_root_hash,
            "Root hash of recovered tree {actual_root_hash:?} differs from expected root hash {:?}. \
             If pruning is enabled and the tree is initialized some time after node recovery, \
             this is caused by snapshot storage logs getting pruned; this setup is currently not supported",
            snapshot.expected_root_hash
        );
        let tree = tree.finalize().await?;
        finalize_latency.observe();
        tracing::info!(
            "Tree recovery has finished, the recovery took {:?}! resuming normal tree operation",
            start_time.elapsed()
        );
        Ok(Some(tree))
    }

    /// Filters out `key_chunks` for which recovery was successfully performed.
    async fn filter_chunks(
        &mut self,
        storage: &mut Connection<'_, Core>,
        snapshot_l2_block: L2BlockNumber,
        key_chunks: &[ops::RangeInclusive<H256>],
    ) -> anyhow::Result<Vec<ops::RangeInclusive<H256>>> {
        let chunk_starts_latency =
            RECOVERY_METRICS.latency[&RecoveryStage::LoadChunkStarts].start();
        let chunk_starts = storage
            .storage_logs_dal()
            .get_chunk_starts_for_l2_block(snapshot_l2_block, key_chunks)
            .await?;
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
            .map(|(_, start_entry)| start_entry.tree_key())
            .collect();
        let tree_entries = self.entries(start_keys).await;

        let mut output = vec![];
        for (tree_entry, (i, db_entry)) in tree_entries.into_iter().zip(existing_starts) {
            if tree_entry.is_empty() {
                output.push(key_chunks[i].clone());
                continue;
            }
            anyhow::ensure!(
                tree_entry.value == db_entry.value && tree_entry.leaf_index == db_entry.leaf_index,
                "Mismatch between entry for key {:0>64x} in Postgres snapshot for L2 block #{snapshot_l2_block} \
                 ({db_entry:?}) and tree ({tree_entry:?}); the recovery procedure may be corrupted",
                db_entry.key
            );
        }
        Ok(output)
    }

    /// Returns `Ok(true)` if the chunk was recovered, `Ok(false)` if the recovery process was interrupted.
    async fn recover_key_chunk(
        tree: &Mutex<AsyncTreeRecovery>,
        snapshot_l2_block: L2BlockNumber,
        key_chunk: ops::RangeInclusive<H256>,
        pool: &ConnectionPool<Core>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<bool> {
        let acquire_connection_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::AcquireConnection].start();
        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        acquire_connection_latency.observe();

        if *stop_receiver.borrow() {
            return Ok(false);
        }

        let entries_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LoadEntries].start();
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_l2_block(snapshot_l2_block, key_chunk.clone())
            .await?;
        drop(storage);
        let entries_latency = entries_latency.observe();
        tracing::debug!(
            "Loaded {} entries for chunk {key_chunk:?} in {entries_latency:?}",
            all_entries.len()
        );

        if *stop_receiver.borrow() {
            return Ok(false);
        }

        // Sanity check: all entry keys must be distinct. Otherwise, we may end up writing non-final values
        // to the tree, since we don't enforce any ordering on entries besides by the hashed key.
        for window in all_entries.windows(2) {
            let [prev_entry, next_entry] = window else {
                unreachable!();
            };
            anyhow::ensure!(
                prev_entry.key != next_entry.key,
                "node snapshot in Postgres is corrupted: entries {prev_entry:?} and {next_entry:?} \
                 have same hashed_key"
            );
        }

        let all_entries = all_entries
            .into_iter()
            .map(|entry| TreeEntry {
                key: entry.tree_key(),
                value: entry.value,
                leaf_index: entry.leaf_index,
            })
            .collect();
        let lock_tree_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LockTree].start();
        let mut tree = tree.lock().await;
        lock_tree_latency.observe();

        if *stop_receiver.borrow() {
            return Ok(false);
        }

        let extend_tree_latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::ExtendTree].start();
        tree.extend(all_entries).await?;
        let extend_tree_latency = extend_tree_latency.observe();
        tracing::debug!(
            "Extended Merkle tree with entries for chunk {key_chunk:?} in {extend_tree_latency:?}"
        );
        Ok(true)
    }
}

async fn get_snapshot_recovery(
    pool: &ConnectionPool<Core>,
) -> anyhow::Result<Option<SnapshotRecoveryStatus>> {
    let mut storage = pool.connection_tagged("metadata_calculator").await?;
    Ok(storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?)
}

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
};

use anyhow::Context as _;
use async_trait::async_trait;
use futures::future;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, Semaphore};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_merkle_tree::TreeEntry;
use zksync_types::{
    snapshots::{uniform_hashed_keys_chunk, SnapshotRecoveryStatus},
    MiniblockNumber, H256,
};

use super::{
    helpers::{AsyncTree, AsyncTreeRecovery, GenericAsyncTree},
    metrics::{ChunkRecoveryStage, RecoveryStage, RECOVERY_METRICS},
};

#[cfg(test)]
mod tests;

/// Handler of recovery life cycle events. This functionality is encapsulated in a trait to be able
/// to control recovery behavior in tests.
#[async_trait]
trait HandleRecoveryEvent: fmt::Debug + Send + Sync {
    fn recovery_started(&mut self, _chunk_count: u64, _recovered_chunk_count: u64) {
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
    chunk_count: u64,
    recovered_chunk_count: u64,
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

#[async_trait]
impl HandleRecoveryEvent for RecoveryHealthUpdater<'_> {
    fn recovery_started(&mut self, chunk_count: u64, recovered_chunk_count: u64) {
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

#[derive(Debug, Clone, Copy)]
struct SnapshotParameters {
    miniblock: MiniblockNumber,
    expected_root_hash: H256,
    log_count: u64,
}

impl SnapshotParameters {
    /// This is intentionally not configurable because chunks must be the same for the entire recovery
    /// (i.e., not changed after a node restart).
    const DESIRED_CHUNK_SIZE: u64 = 200_000;

    async fn new(pool: &ConnectionPool, recovery: &SnapshotRecoveryStatus) -> anyhow::Result<Self> {
        let miniblock = recovery.miniblock_number;
        let expected_root_hash = recovery.l1_batch_root_hash;

        let mut storage = pool.access_storage().await?;
        let log_count = storage
            .storage_logs_dal()
            .count_miniblock_storage_logs(miniblock)
            .await
            .with_context(|| format!("Failed getting number of logs for miniblock #{miniblock}"))?;

        Ok(Self {
            miniblock,
            expected_root_hash,
            log_count,
        })
    }

    fn chunk_count(&self) -> u64 {
        self.log_count.div_ceil(Self::DESIRED_CHUNK_SIZE)
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
    pub async fn ensure_ready(
        self,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
        health_updater: &HealthUpdater,
    ) -> anyhow::Result<Option<AsyncTree>> {
        let (tree, snapshot_recovery) = match self {
            Self::Ready(tree) => return Ok(Some(tree)),
            Self::Recovering(tree) => {
                let snapshot_recovery = get_snapshot_recovery(pool).await?.context(
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
                if let Some(snapshot_recovery) = get_snapshot_recovery(pool).await? {
                    tracing::info!(
                        "Starting Merkle tree recovery with status {snapshot_recovery:?}"
                    );
                    let l1_batch = snapshot_recovery.l1_batch_number;
                    let tree = AsyncTreeRecovery::new(db, l1_batch.0.into(), mode);
                    (tree, snapshot_recovery)
                } else {
                    // Start the tree from scratch. The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    return Ok(Some(AsyncTree::new(db, mode)));
                }
            }
        };

        let snapshot = SnapshotParameters::new(pool, &snapshot_recovery).await?;
        tracing::debug!("Obtained snapshot parameters: {snapshot:?}");
        let recovery_options = RecoveryOptions {
            chunk_count: snapshot.chunk_count(),
            concurrency_limit: pool.max_size() as usize,
            events: Box::new(RecoveryHealthUpdater::new(health_updater)),
        };
        tree.recover(snapshot, recovery_options, pool, stop_receiver)
            .await
    }
}

impl AsyncTreeRecovery {
    async fn recover(
        mut self,
        snapshot: SnapshotParameters,
        mut options: RecoveryOptions<'_>,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<AsyncTree>> {
        let chunk_count = options.chunk_count;
        let chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        tracing::info!(
            "Recovering Merkle tree from Postgres snapshot in {chunk_count} concurrent chunks"
        );

        let mut storage = pool.access_storage().await?;
        let remaining_chunks = self
            .filter_chunks(&mut storage, snapshot.miniblock, &chunks)
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
            options.events.chunk_started().await;
            Self::recover_key_chunk(&tree, snapshot.miniblock, chunk, pool, stop_receiver).await?;
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
            actual_root_hash == snapshot.expected_root_hash,
            "Root hash of recovered tree {actual_root_hash:?} differs from expected root hash {:?}",
            snapshot.expected_root_hash
        );
        let tree = tree.finalize().await;
        let finalize_latency = finalize_latency.observe();
        tracing::info!(
            "Finished tree recovery in {finalize_latency:?}; resuming normal tree operation"
        );
        Ok(Some(tree))
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
                "Mismatch between entry for key {:0>64x} in Postgres snapshot for miniblock #{snapshot_miniblock} \
                 ({db_entry:?}) and tree ({tree_entry:?}); the recovery procedure may be corrupted",
                db_entry.key
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
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone())
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

async fn get_snapshot_recovery(
    pool: &ConnectionPool,
) -> anyhow::Result<Option<SnapshotRecoveryStatus>> {
    let mut storage = pool.access_storage_tagged("metadata_calculator").await?;
    Ok(storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?)
}

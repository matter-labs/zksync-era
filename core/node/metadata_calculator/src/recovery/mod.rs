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
    fmt,
    num::NonZeroU32,
    ops,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use anyhow::Context as _;
use async_trait::async_trait;
use futures::future;
use tokio::sync::{watch, Mutex, Semaphore};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_health_check::HealthUpdater;
use zksync_merkle_tree::TreeEntry;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_types::{
    snapshots::uniform_hashed_keys_chunk, L1BatchNumber, L2BlockNumber, OrStopped, H256,
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
#[async_trait]
trait HandleRecoveryEvent: fmt::Debug + Send + Sync {
    fn recovery_started(&mut self, _chunk_count: u64, _recovered_chunk_count: u64) {
        // Default implementation does nothing
    }

    async fn chunk_recovered(&self) {
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
struct InitParameters {
    l1_batch: L1BatchNumber,
    l2_block: L2BlockNumber,
    expected_root_hash: Option<H256>,
    log_count: u64,
    desired_chunk_size: u64,
}

impl InitParameters {
    /// Minimum number of storage logs in the genesis state to initiate recovery.
    const MIN_STORAGE_LOGS_FOR_GENESIS_RECOVERY: u32 = if cfg!(test) {
        // Select the smaller threshold for tests, but make it large enough so that it's not triggered by unrelated tests.
        1_000
    } else {
        100_000
    };

    async fn new(
        pool: &ConnectionPool<Core>,
        config: &MetadataCalculatorRecoveryConfig,
    ) -> anyhow::Result<Option<Self>> {
        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        let recovery_status = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?;
        let pruning_info = storage.pruning_dal().get_pruning_info().await?;

        let (l1_batch, l2_block);
        let mut expected_root_hash = None;
        match (recovery_status, pruning_info.last_hard_pruned) {
            (Some(recovery), None) => {
                tracing::warn!(
                    "Snapshot recovery {recovery:?} is present on the node, but pruning info is empty; assuming no pruning happened"
                );
                l1_batch = recovery.l1_batch_number;
                l2_block = recovery.l2_block_number;
                expected_root_hash = Some(recovery.l1_batch_root_hash);
            }
            (Some(recovery), Some(pruned)) => {
                // We have both recovery and some pruning on top of it.
                l2_block = pruned.l2_block.max(recovery.l2_block_number);
                l1_batch = pruned.l1_batch;
                if let Some(root_hash) = pruned.l1_batch_root_hash {
                    expected_root_hash = Some(root_hash);
                } else if l1_batch == recovery.l1_batch_number {
                    expected_root_hash = Some(recovery.l1_batch_root_hash);
                }
            }
            (None, Some(pruned)) => {
                l2_block = pruned.l2_block;
                l1_batch = pruned.l1_batch;
                expected_root_hash = pruned.l1_batch_root_hash;
            }
            (None, None) => {
                // Check whether we need recovery for the genesis state. This could be necessary if the genesis state
                // for the chain is very large (order of millions of entries).
                let is_genesis_recovery_needed =
                    Self::is_genesis_recovery_needed(&mut storage).await?;
                if is_genesis_recovery_needed {
                    l2_block = L2BlockNumber(0);
                    l1_batch = L1BatchNumber(0);
                    expected_root_hash = storage
                        .blocks_dal()
                        .get_l1_batch_state_root(l1_batch)
                        .await?;
                } else {
                    return Ok(None);
                }
            }
        };

        let log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(l2_block)
            .await?;

        Ok(Some(Self {
            l1_batch,
            l2_block,
            expected_root_hash,
            log_count,
            desired_chunk_size: config.desired_chunk_size,
        }))
    }

    #[tracing::instrument(skip_all, ret)]
    async fn is_genesis_recovery_needed(
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<bool> {
        let earliest_l2_block = storage.blocks_dal().get_earliest_l2_block_number().await?;
        tracing::debug!(?earliest_l2_block, "Got earliest L2 block from Postgres");
        if earliest_l2_block != Some(L2BlockNumber(0)) {
            tracing::info!(
                ?earliest_l2_block,
                "There is no genesis block in Postgres; genesis recovery is impossible"
            );
            return Ok(false);
        }

        // We get the full log count later, but for now do a faster query to understand the approximate amount.
        storage
            .storage_logs_dal()
            .check_storage_log_count(
                L2BlockNumber(0),
                NonZeroU32::new(Self::MIN_STORAGE_LOGS_FOR_GENESIS_RECOVERY).unwrap(),
            )
            .await
            .map_err(Into::into)
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
    ) -> Result<AsyncTree, OrStopped> {
        let started_at = Instant::now();
        let (tree, init_params) = match self {
            Self::Ready(tree) => return Ok(tree),
            Self::Recovering(tree) => {
                let params = InitParameters::new(main_pool, config).await?.context(
                    "Merkle tree is recovering, but Postgres doesn't contain snapshot recovery information",
                )?;

                let recovered_version = tree.recovered_version();
                if u64::from(params.l1_batch.0) != recovered_version {
                    let err = anyhow::anyhow!(
                        "Snapshot L1 batch in Postgres ({params:?}) differs from the recovered Merkle tree version \
                         ({recovered_version})"
                    );
                    return Err(err.into());
                }

                tracing::info!("Resuming tree recovery with status: {params:?}");
                (tree, params)
            }
            Self::Empty { db, mode } => {
                if let Some(params) = InitParameters::new(main_pool, config).await? {
                    tracing::info!("Starting Merkle tree recovery with status {params:?}");
                    let l1_batch = params.l1_batch;
                    let tree = AsyncTreeRecovery::new(db, l1_batch.0.into(), mode, config)?;
                    (tree, params)
                } else {
                    // The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    tracing::info!("Starting Merkle tree from scratch");
                    return Ok(AsyncTree::new(db, mode)?);
                }
            }
        };

        tracing::debug!(
            "Obtained recovery init parameters: {init_params:?} based on recovery configuration {config:?}"
        );
        let recovery_options = RecoveryOptions {
            chunk_count: init_params.chunk_count(),
            concurrency_limit: recovery_pool.max_size() as usize,
            events: Box::new(RecoveryHealthUpdater::new(health_updater)),
        };
        let tree = tree
            .recover(init_params, recovery_options, &recovery_pool, stop_receiver)
            .await?;
        // Only report latency if recovery wasn't canceled
        let elapsed = started_at.elapsed();
        APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::Tree].set(elapsed);
        tracing::info!("Recovered Merkle tree from snapshot in {elapsed:?}");
        Ok(tree)
    }
}

impl AsyncTreeRecovery {
    async fn recover(
        mut self,
        init_params: InitParameters,
        mut options: RecoveryOptions<'_>,
        pool: &ConnectionPool<Core>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<AsyncTree, OrStopped> {
        self.ensure_desired_chunk_size(init_params.desired_chunk_size)
            .await?;

        let start_time = Instant::now();
        let chunk_count = options.chunk_count;
        let chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        tracing::info!(
            "Recovering Merkle tree from Postgres snapshot in {chunk_count} chunks with max concurrency {}. \
             Be aware that enabling node pruning during recovery will probably result in a recovery error; always disable pruning \
             until recovery is complete",
            options.concurrency_limit
        );

        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        let remaining_chunks = self
            .filter_chunks(&mut storage, init_params.l2_block, &chunks)
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
            if Self::recover_key_chunk(&tree, init_params.l2_block, chunk, pool, stop_receiver)
                .await?
            {
                options.events.chunk_recovered().await;
            }
            anyhow::Ok(())
        });
        future::try_join_all(chunk_tasks).await?;

        let mut tree = tree.into_inner();
        if *stop_receiver.borrow() {
            // Waiting for persistence is mostly useful for tests. Normally, the tree database won't be used in the same process
            // after a stop request is received, so there's no risk of data races with the background persistence thread.
            tree.wait_for_persistence().await?;
            return Err(OrStopped::Stopped);
        }

        let finalize_latency = RECOVERY_METRICS.latency[&RecoveryStage::Finalize].start();
        let actual_root_hash = tree.root_hash().await;
        if let Some(expected_root_hash) = init_params.expected_root_hash {
            if actual_root_hash != expected_root_hash {
                let err = anyhow::anyhow!(
                    "Root hash of recovered tree {actual_root_hash:?} differs from expected root hash {expected_root_hash:?}"
                );
                return Err(err.into());
            }
        }

        // Check pruning info one last time before finalizing the tree.
        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        Self::check_pruning_info(&mut storage, init_params.l2_block).await?;
        drop(storage);

        let tree = tree.finalize().await?;
        finalize_latency.observe();
        tracing::info!(
            "Tree recovery has finished, the recovery took {:?}! resuming normal tree operation",
            start_time.elapsed()
        );
        Ok(tree)
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

    async fn check_pruning_info(
        storage: &mut Connection<'_, Core>,
        snapshot_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        let pruning_info = storage.pruning_dal().get_pruning_info().await?;
        if let Some(pruned) = pruning_info.last_hard_pruned {
            anyhow::ensure!(
                pruned.l2_block == snapshot_l2_block,
                "Additional data was pruned compared to tree recovery L2 block #{snapshot_l2_block}: {pruning_info:?}. \
                 Continuing recovery is impossible; to recover the tree, drop its RocksDB directory, stop pruning and restart recovery"
            );
        }
        Ok(())
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
        Self::check_pruning_info(&mut storage, snapshot_l2_block).await?;
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

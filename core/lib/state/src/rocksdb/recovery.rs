//! Logic for [`RocksdbStorage`] related to snapshot recovery.

use std::{num::NonZeroU32, ops};

use anyhow::Context as _;
use futures::future;
use tokio::sync::{watch, Mutex, Semaphore};
use zksync_dal::{
    storage_logs_dal::StorageRecoveryLogEntry, Connection, ConnectionPool, Core, CoreDal, DalError,
};
use zksync_types::{snapshots::uniform_hashed_keys_chunk, L1BatchNumber, L2BlockNumber, H256};

use super::{
    metrics::{ChunkRecoveryStage, RecoveryStage, RECOVERY_METRICS},
    RocksdbStorage, RocksdbSyncError, StateValue,
};

#[derive(Debug)]
pub enum InitStrategy {
    Complete,
    Recovery,
    Genesis,
}

#[derive(Debug)]
struct KeyChunk {
    id: u64,
    key_range: ops::RangeInclusive<H256>,
    start_entry: Option<StorageRecoveryLogEntry>,
}

#[derive(Debug)]
struct InitParameters {
    l1_batch: L1BatchNumber,
    l2_block: L2BlockNumber,
    desired_log_chunk_size: u64,
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
        storage: &mut Connection<'_, Core>,
        desired_log_chunk_size: u64,
    ) -> anyhow::Result<Option<Self>> {
        let recovery_status = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?;
        let pruning_info = storage.pruning_dal().get_pruning_info().await?;
        tracing::debug!(
            ?recovery_status,
            ?pruning_info,
            "Fetched snapshot / pruning info"
        );

        let (l1_batch, l2_block) = match (recovery_status, pruning_info.last_hard_pruned) {
            (Some(recovery), None) => {
                tracing::warn!(
                    "Snapshot recovery {recovery:?} is present on the node, but pruning info is empty; assuming no pruning happened"
                );
                (recovery.l1_batch_number, recovery.l2_block_number)
            }
            (Some(recovery), Some(pruned)) => {
                // We have both recovery and some pruning on top of it.
                (
                    pruned.l1_batch,
                    pruned.l2_block.max(recovery.l2_block_number),
                )
            }
            (None, Some(pruned)) => (pruned.l1_batch, pruned.l2_block),
            (None, None) => {
                // Check whether we need recovery for the genesis state. This could be necessary if the genesis state
                // for the chain is very large (order of millions of entries).
                let is_genesis_recovery_needed = Self::is_genesis_recovery_needed(storage).await?;
                if !is_genesis_recovery_needed {
                    return Ok(None);
                }
                (L1BatchNumber(0), L2BlockNumber(0))
            }
        };
        Ok(Some(Self {
            l1_batch,
            l2_block,
            desired_log_chunk_size,
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

        storage
            .storage_logs_dal()
            .check_storage_log_count(
                L2BlockNumber(0),
                NonZeroU32::new(Self::MIN_STORAGE_LOGS_FOR_GENESIS_RECOVERY).unwrap(),
            )
            .await
            .map_err(Into::into)
    }
}

impl RocksdbStorage {
    /// Ensures that this storage is ready for normal operation (i.e., updates by L1 batch).
    ///
    /// # Return value
    ///
    /// Returns the next L1 batch that should be fed to the storage.
    #[tracing::instrument(skip_all)]
    pub(super) async fn ensure_ready(
        &mut self,
        pool: &ConnectionPool<Core>,
        desired_log_chunk_size: u64,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(InitStrategy, L1BatchNumber), RocksdbSyncError> {
        if let Some(l1_batch_number) = self.next_l1_batch_number_opt().await {
            tracing::trace!(?l1_batch_number, "RocksDB storage is ready");
            return Ok((InitStrategy::Complete, l1_batch_number));
        }

        let mut storage = pool
            .connection_tagged("state_keeper")
            .await
            .map_err(DalError::generalize)?;
        let init_params = InitParameters::new(&mut storage, desired_log_chunk_size).await?;
        drop(storage);

        if let Some(recovery_batch_number) = self.recovery_l1_batch_number().await? {
            tracing::info!(?recovery_batch_number, "Resuming storage recovery");
            let init_params = init_params.as_ref().context(
                "Storage is recovering, but Postgres no longer contains necessary metadata",
            )?;
            if recovery_batch_number != init_params.l1_batch {
                let err = anyhow::anyhow!(
                    "Snapshot parameters in Postgres ({init_params:?}) differ from L1 batch for the recovered storage \
                    ({recovery_batch_number})"
                );
                return Err(err.into());
            }
        }

        Ok(if let Some(init_params) = init_params {
            self.recover_from_snapshot(pool, &init_params, stop_receiver)
                .await?;
            (InitStrategy::Recovery, init_params.l1_batch + 1)
        } else {
            tracing::info!("Initializing RocksDB storage from genesis");
            self.set_l1_batch_number(L1BatchNumber(0), false).await?;
            (InitStrategy::Genesis, L1BatchNumber(0))
        })
    }

    /// # Important
    ///
    /// `Self::L1_BATCH_NUMBER_KEY` must be set at the very end of the process. If it is set earlier, recovery is not fault-tolerant
    /// (it would be considered complete even if it failed in the middle).
    async fn recover_from_snapshot(
        &mut self,
        pool: &ConnectionPool<Core>,
        init_parameters: &InitParameters,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), RocksdbSyncError> {
        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }

        tracing::info!(
            ?init_parameters,
            concurrency = pool.max_size(),
            "Recovering secondary storage from snapshot"
        );

        let mut storage = pool.connection_tagged("state_keeper").await?;
        self.set_l1_batch_number(init_parameters.l1_batch, true)
            .await?;
        self.recover_factory_deps(&mut storage, init_parameters)
            .await?;

        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }
        let key_chunks = Self::load_key_chunks(&mut storage, init_parameters).await?;
        drop(storage);

        let semaphore = Semaphore::new(pool.max_size() as usize);
        let total_chunk_count = key_chunks.len();
        let key_chunks = self.filter_key_chunks(key_chunks, init_parameters).await?;
        let recovered_chunk_count = total_chunk_count - key_chunks.len();
        RECOVERY_METRICS
            .recovered_chunk_count
            .set(recovered_chunk_count);
        tracing::info!(
            "Checked chunk starts; {recovered_chunk_count} chunks are already recovered"
        );

        let db_mutex = Mutex::new(self.clone());
        let chunk_tasks = key_chunks.into_iter().map(|chunk| async {
            let _permit = semaphore
                .acquire()
                .await
                .context("semaphore is never closed")?;
            Self::recover_logs_chunk(
                &db_mutex,
                pool,
                init_parameters,
                chunk,
                total_chunk_count,
                stop_receiver,
            )
            .await
        });
        future::try_join_all(chunk_tasks).await?;
        drop(db_mutex);

        tracing::info!("All chunks recovered; finalizing recovery process");
        self.save(Some(init_parameters.l1_batch + 1)).await?;
        Ok(())
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut Connection<'_, Core>,
        init_parameters: &InitParameters,
    ) -> anyhow::Result<()> {
        // We don't expect that many factory deps; that's why we recover factory deps in any case.
        let latency = RECOVERY_METRICS.latency[&RecoveryStage::LoadFactoryDeps].start();
        let factory_deps = storage
            .snapshots_creator_dal()
            .get_all_factory_deps(init_parameters.l2_block)
            .await?;
        let latency = latency.observe();
        tracing::info!(
            "Loaded {} factory dependencies from the snapshot in {latency:?}",
            factory_deps.len()
        );

        let latency = RECOVERY_METRICS.latency[&RecoveryStage::SaveFactoryDeps].start();
        for (bytecode_hash, bytecode) in factory_deps {
            self.store_factory_dep(bytecode_hash, bytecode);
        }
        self.save(None)
            .await
            .context("failed saving factory deps")?;
        let latency = latency.observe();
        tracing::info!("Saved factory dependencies to RocksDB in {latency:?}");
        Ok(())
    }

    async fn load_key_chunks(
        storage: &mut Connection<'_, Core>,
        init_parameters: &InitParameters,
    ) -> anyhow::Result<Vec<KeyChunk>> {
        let snapshot_l2_block = init_parameters.l2_block;
        let log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(snapshot_l2_block)
            .await
            .with_context(|| {
                format!("Failed getting number of logs for L2 block #{snapshot_l2_block}")
            })?;
        let chunk_count = log_count.div_ceil(init_parameters.desired_log_chunk_size);
        tracing::info!(
            "Estimated the number of chunks for recovery based on {log_count} logs: {chunk_count}"
        );

        let latency = RECOVERY_METRICS.latency[&RecoveryStage::LoadChunkStarts].start();
        let key_chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        let chunk_starts = storage
            .storage_logs_dal()
            .get_chunk_starts_for_l2_block(snapshot_l2_block, &key_chunks)
            .await?;
        let latency = latency.observe();
        tracing::info!("Loaded {chunk_count} chunk starts in {latency:?}");

        let key_chunks = (0..chunk_count)
            .zip(key_chunks)
            .zip(chunk_starts)
            .map(|((id, key_range), start_entry)| KeyChunk {
                id,
                key_range,
                start_entry,
            })
            .collect();
        Ok(key_chunks)
    }

    async fn read_state_value_async(&self, hashed_key: H256) -> Option<StateValue> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || Self::read_state_value(&db, hashed_key))
            .await
            .unwrap()
    }

    async fn check_pruning_info(
        storage: &mut Connection<'_, Core>,
        snapshot_l1_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let pruning_info = storage.pruning_dal().get_pruning_info().await?;
        if let Some(pruned) = pruning_info.last_hard_pruned {
            // Unlike with the tree, we check L1 batch number since it's retained in the cache RocksDB across restarts.
            anyhow::ensure!(
                pruned.l1_batch == snapshot_l1_batch,
                "Additional data was pruned compared to recovery L1 batch #{snapshot_l1_batch}: {pruning_info:?}. \
                 Continuing recovery is impossible; to recover the cache, drop its RocksDB directory, stop pruning and restart recovery"
            );
        }
        Ok(())
    }

    async fn filter_key_chunks(
        &self,
        key_chunks: Vec<KeyChunk>,
        init_parameters: &InitParameters,
    ) -> anyhow::Result<Vec<KeyChunk>> {
        let mut retained_chunks = vec![];
        for key_chunk in key_chunks {
            let chunk_id = key_chunk.id;
            let Some(chunk_start) = key_chunk.start_entry else {
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) doesn't have entries in Postgres; skipping");
                continue;
            };

            // Check whether the chunk is already recovered.
            let state_value = self.read_state_value_async(chunk_start.key).await;
            if let Some(state_value) = state_value {
                if state_value.value != chunk_start.value
                    || state_value.enum_index != Some(chunk_start.leaf_index)
                {
                    anyhow::bail!(
                        "Mismatch between entry for key {:?} in Postgres snapshot for L2 block #{} \
                         ({chunk_start:?}) and RocksDB cache ({state_value:?}); the recovery procedure may be corrupted",
                        chunk_start.key,
                        init_parameters.l1_batch
                    );
                }
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) is already recovered; skipping");
            } else {
                retained_chunks.push(key_chunk);
            }
        }
        Ok(retained_chunks)
    }

    #[tracing::instrument(skip_all, fields(id = key_chunk.id, range = ?key_chunk.key_range))]
    async fn recover_logs_chunk(
        this: &Mutex<Self>,
        pool: &ConnectionPool<Core>,
        init_parameters: &InitParameters,
        key_chunk: KeyChunk,
        total_chunk_count: usize,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), RocksdbSyncError> {
        let latency =
            RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::AcquireConnection].start();
        let mut storage = pool.connection_tagged("state_keeper").await?;
        latency.observe();

        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }

        let latency = RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LoadEntries].start();
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_l2_block(init_parameters.l2_block, key_chunk.key_range.clone())
            .await?;
        let latency = latency.observe();
        tracing::debug!(?latency, len = all_entries.len(), "Loaded log entries");

        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }

        Self::check_pruning_info(&mut storage, init_parameters.l1_batch).await?;
        drop(storage);

        let latency = RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LockDb].start();
        let mut this = this.lock().await;
        let latency = latency.observe();
        tracing::debug!(?latency, "Acquired RocksDB mutex");

        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }

        let latency = RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::SaveEntries].start();
        this.pending_patch.state = all_entries
            .into_iter()
            .map(|entry| (entry.key, (entry.value, entry.leaf_index)))
            .collect();
        this.save(None)
            .await
            .context("failed saving storage logs chunk")?;
        let latency = latency.observe();
        tracing::debug!(?latency, "Saved logs");

        let recovered_chunk_count = RECOVERY_METRICS.recovered_chunk_count.inc_by(1) + 1;
        let chunks_left = total_chunk_count.saturating_sub(recovered_chunk_count);
        tracing::info!(
            "Recovered {recovered_chunk_count}/{total_chunk_count} RocksDB cache chunks, there are {chunks_left} left to process",
        );

        #[cfg(test)]
        this.listener
            .on_logs_chunk_recovered
            .handle(key_chunk.id)
            .await;
        Ok(())
    }
}

//! Logic for [`RocksdbStorage`] related to snapshot recovery.

use std::ops;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{storage_logs_dal::StorageRecoveryLogEntry, Connection, Core, CoreDal, DalError};
use zksync_types::{
    snapshots::{uniform_hashed_keys_chunk, SnapshotRecoveryStatus},
    L1BatchNumber, MiniblockNumber, H256,
};

use super::{
    metrics::{ChunkRecoveryStage, RecoveryStage, RECOVERY_METRICS},
    RocksdbStorage, RocksdbSyncError, StateValue,
};

#[derive(Debug)]
struct KeyChunk {
    id: u64,
    key_range: ops::RangeInclusive<H256>,
    start_entry: Option<StorageRecoveryLogEntry>,
}

impl RocksdbStorage {
    /// Ensures that this storage is ready for normal operation (i.e., updates by L1 batch).
    ///
    /// # Return value
    ///
    /// Returns the next L1 batch that should be fed to the storage.
    pub(super) async fn ensure_ready(
        &mut self,
        storage: &mut Connection<'_, Core>,
        desired_log_chunk_size: u64,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<L1BatchNumber, RocksdbSyncError> {
        if let Some(number) = self.l1_batch_number().await {
            return Ok(number);
        }

        // Check whether we need to perform a snapshot migration.
        let snapshot_recovery = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .map_err(DalError::generalize)?;
        Ok(if let Some(snapshot_recovery) = snapshot_recovery {
            self.recover_from_snapshot(
                storage,
                &snapshot_recovery,
                desired_log_chunk_size,
                stop_receiver,
            )
            .await?;
            snapshot_recovery.l1_batch_number + 1
        } else {
            // No recovery snapshot; we're initializing the cache from the genesis
            L1BatchNumber(0)
        })
    }

    /// # Important
    ///
    /// `Self::L1_BATCH_NUMBER_KEY` must be set at the very end of the process. If it is set earlier, recovery is not fault-tolerant
    /// (it would be considered complete even if it failed in the middle).
    async fn recover_from_snapshot(
        &mut self,
        storage: &mut Connection<'_, Core>,
        snapshot_recovery: &SnapshotRecoveryStatus,
        desired_log_chunk_size: u64,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), RocksdbSyncError> {
        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }
        tracing::info!("Recovering secondary storage from snapshot: {snapshot_recovery:?}");

        self.recover_factory_deps(storage, snapshot_recovery)
            .await?;

        if *stop_receiver.borrow() {
            return Err(RocksdbSyncError::Interrupted);
        }
        let key_chunks =
            Self::load_key_chunks(storage, snapshot_recovery, desired_log_chunk_size).await?;

        RECOVERY_METRICS.recovered_chunk_count.set(0);
        for key_chunk in key_chunks {
            if *stop_receiver.borrow() {
                return Err(RocksdbSyncError::Interrupted);
            }

            let chunk_id = key_chunk.id;
            let Some(chunk_start) = key_chunk.start_entry else {
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) doesn't have entries in Postgres; skipping");
                RECOVERY_METRICS.recovered_chunk_count.inc_by(1);
                continue;
            };

            // Check whether the chunk is already recovered.
            let state_value = self.read_state_value_async(chunk_start.key).await;
            if let Some(state_value) = state_value {
                if state_value.value != chunk_start.value
                    || state_value.enum_index != Some(chunk_start.leaf_index)
                {
                    let err = anyhow::anyhow!(
                        "Mismatch between entry for key {:?} in Postgres snapshot for miniblock #{} \
                         ({chunk_start:?}) and RocksDB cache ({state_value:?}); the recovery procedure may be corrupted",
                        chunk_start.key,
                        snapshot_recovery.miniblock_number
                    );
                    return Err(err.into());
                }
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) is already recovered; skipping");
            } else {
                self.recover_logs_chunk(
                    storage,
                    snapshot_recovery.miniblock_number,
                    key_chunk.key_range.clone(),
                )
                .await
                .with_context(|| {
                    format!(
                        "failed recovering logs chunk {chunk_id} (hashed key range {:?})",
                        key_chunk.key_range
                    )
                })?;

                #[cfg(test)]
                (self.listener.on_logs_chunk_recovered)(chunk_id);
            }
            RECOVERY_METRICS.recovered_chunk_count.inc_by(1);
        }

        tracing::info!("All chunks recovered; finalizing recovery process");
        self.save(Some(snapshot_recovery.l1_batch_number + 1))
            .await?;
        Ok(())
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut Connection<'_, Core>,
        snapshot_recovery: &SnapshotRecoveryStatus,
    ) -> anyhow::Result<()> {
        // We don't expect that many factory deps; that's why we recover factory deps in any case.
        let latency = RECOVERY_METRICS.latency[&RecoveryStage::LoadFactoryDeps].start();
        let factory_deps = storage
            .snapshots_creator_dal()
            .get_all_factory_deps(snapshot_recovery.miniblock_number)
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
        snapshot_recovery: &SnapshotRecoveryStatus,
        desired_log_chunk_size: u64,
    ) -> anyhow::Result<Vec<KeyChunk>> {
        let snapshot_miniblock = snapshot_recovery.miniblock_number;
        let log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(snapshot_miniblock)
            .await
            .with_context(|| {
                format!("Failed getting number of logs for miniblock #{snapshot_miniblock}")
            })?;
        let chunk_count = log_count.div_ceil(desired_log_chunk_size);
        tracing::info!(
            "Estimated the number of chunks for recovery based on {log_count} logs: {chunk_count}"
        );

        let latency = RECOVERY_METRICS.latency[&RecoveryStage::LoadChunkStarts].start();
        let key_chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        let chunk_starts = storage
            .storage_logs_dal()
            .get_chunk_starts_for_miniblock(snapshot_miniblock, &key_chunks)
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

    async fn recover_logs_chunk(
        &mut self,
        storage: &mut Connection<'_, Core>,
        snapshot_miniblock: MiniblockNumber,
        key_chunk: ops::RangeInclusive<H256>,
    ) -> anyhow::Result<()> {
        let latency = RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::LoadEntries].start();
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone())
            .await?;
        let latency = latency.observe();
        tracing::debug!(
            "Loaded {} log entries for chunk {key_chunk:?} in {latency:?}",
            all_entries.len()
        );

        let latency = RECOVERY_METRICS.chunk_latency[&ChunkRecoveryStage::SaveEntries].start();
        self.pending_patch.state = all_entries
            .into_iter()
            .map(|entry| (entry.key, (entry.value, entry.leaf_index)))
            .collect();
        self.save(None)
            .await
            .context("failed saving storage logs chunk")?;
        let latency = latency.observe();
        tracing::debug!("Saved logs chunk {key_chunk:?} to RocksDB in {latency:?}");

        tracing::info!("Recovered hashed key chunk {key_chunk:?}");
        Ok(())
    }
}

//! Logic for [`RocksdbStorage`] related to snapshot recovery.

use std::ops;

use anyhow::Context as _;
use zksync_dal::StorageProcessor;
use zksync_types::{
    snapshots::{uniform_hashed_keys_chunk, SnapshotRecoveryStatus},
    L1BatchNumber, MiniblockNumber, H256,
};
use zksync_utils::u256_to_h256;

use super::RocksdbStorage;

impl RocksdbStorage {
    /// Ensures that this storage is ready for normal operation (i.e., updates by L1 batch).
    pub(super) async fn ensure_ready(
        &mut self,
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(if let Some(number) = self.l1_batch_number().await {
            number
        } else {
            // Check whether we need to perform a snapshot migration.
            let snapshot_recovery = storage
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await
                .context("failed getting snapshot recovery info")?;
            if let Some(snapshot_recovery) = snapshot_recovery {
                self.recover_from_snapshot(storage, &snapshot_recovery)
                    .await
                    .context("failed recovering from snapshot")?;
                snapshot_recovery.l1_batch_number
            } else {
                // No recovery snapshot; we're initializing the cache from the genesis
                L1BatchNumber(0)
            }
        })
    }

    // FIXME: `stop_receiver`?
    /// # Important
    ///
    /// `Self::L1_BATCH_NUMBER_KEY` must be set at the very end of the process. If it is set earlier, recovery is not fault-tolerant
    /// (it would be considered complete even if it failed in the middle).
    async fn recover_from_snapshot(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        snapshot_recovery: &SnapshotRecoveryStatus,
    ) -> anyhow::Result<()> {
        /// This is intentionally not configurable because chunks must be the same for the entire recovery
        /// (i.e., not changed after a node restart).
        const DESIRED_CHUNK_SIZE: u64 = 200_000;

        tracing::info!("Recovering secondary storage from snapshot: {snapshot_recovery:?}");

        // We don't expect that many factory deps; that's why we recover factory deps in any case.
        let factory_deps = storage
            .blocks_dal()
            .get_l1_batch_factory_deps(snapshot_recovery.l1_batch_number)
            .await
            .context("Failed getting factory dependencies")?;
        tracing::info!(
            "Loaded {} factory dependencies from the snapshot",
            factory_deps.len()
        );
        for (bytecode_hash, bytecode) in factory_deps {
            self.store_factory_dep(bytecode_hash, bytecode);
        }
        self.save(None).await;

        let snapshot_miniblock = snapshot_recovery.miniblock_number;
        let log_count = storage
            .storage_logs_dal()
            .count_miniblock_storage_logs(snapshot_miniblock)
            .await
            .with_context(|| {
                format!("Failed getting number of logs for miniblock #{snapshot_miniblock}")
            })?;
        let chunk_count = zksync_utils::ceil_div(log_count, DESIRED_CHUNK_SIZE);
        tracing::info!(
            "Estimated the number of chunks for recovery based on {log_count} logs: {chunk_count}"
        );

        let key_chunks: Vec<_> = (0..chunk_count)
            .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunk_count))
            .collect();
        let chunk_starts = storage
            .storage_logs_dal()
            .get_chunk_starts_for_miniblock(snapshot_miniblock, &key_chunks)
            .await
            .context("Failed getting chunk starts")?;

        for ((chunk_id, key_chunk), chunk_start) in
            (0..chunk_count).zip(key_chunks).zip(chunk_starts)
        {
            let Some(chunk_start) = chunk_start else {
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) doesn't have entries in Postgres; skipping");
                continue;
            };

            // Check whether the chunk is already recovered.
            // FIXME: wrap in `spawn_blocking`
            if let Some(state_value) = self.read_state_value(u256_to_h256(chunk_start.key)) {
                anyhow::ensure!(
                    state_value.value == chunk_start.value && state_value.enum_index == Some(chunk_start.leaf_index),
                    "Mismatch between entry for key {:0>64x} in Postgres snapshot for miniblock #{snapshot_miniblock} \
                     ({chunk_start:?}) and RocksDB cache ({state_value:?}); the recovery procedure may be corrupted",
                    chunk_start.key
                );
                tracing::info!("Chunk {chunk_id} (hashed key range {key_chunk:?}) is already recovered; skipping");
            } else {
                self.recover_logs_chunk(
                    storage,
                    snapshot_recovery.miniblock_number,
                    key_chunk.clone(),
                )
                .await
                .with_context(|| {
                    format!(
                        "failed recovering logs chunk {chunk_id} (hashed key range {key_chunk:?})"
                    )
                })?;
            }
        }
        Ok(())
    }

    // TODO: metrics?
    async fn recover_logs_chunk(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        snapshot_miniblock: MiniblockNumber,
        key_chunk: ops::RangeInclusive<H256>,
    ) -> anyhow::Result<()> {
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone())
            .await
            .with_context(|| {
                format!("Failed getting entries for chunk {key_chunk:?} in snapshot for miniblock #{snapshot_miniblock}")
            })?;
        tracing::debug!(
            "Loaded {} log entries for chunk {key_chunk:?}",
            all_entries.len()
        );

        self.pending_patch.state = all_entries
            .into_iter()
            .map(|entry| (u256_to_h256(entry.key), (entry.value, entry.leaf_index)))
            .collect();
        self.save(None).await;

        tracing::info!("Recovered hashed key chunk {key_chunk:?}");
        Ok(())
    }
}

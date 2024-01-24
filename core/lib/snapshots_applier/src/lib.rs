use std::{collections::HashMap, fmt, time::Duration};

use async_trait::async_trait;
use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};
use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    api::en::SyncBlock,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    MiniblockNumber, H256,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::jsonrpsee::core::{client::Error, ClientError as RpcError};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum StorageLogsChunksStage {
    LoadFromGcs,
    SaveToPostgres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum InitialStage {
    FetchMetadataFromMainNode,
    ApplyFactoryDeps,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "snapshots_applier")]
pub(crate) struct SnapshotsApplierMetrics {
    /// Number of chunks in the applied snapshot. Set when snapshots applier starts.
    pub storage_logs_chunks_count: Gauge<usize>,

    /// Number of chunks left to apply.
    pub storage_logs_chunks_left_to_process: Gauge<usize>,

    /// Total latency of applying snapshot.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub snapshot_applying_duration: Histogram<Duration>,

    /// Latency of initial recovery operation split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub initial_stage_duration: Family<InitialStage, Histogram<Duration>>,

    /// Latency of storage log chunk processing split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub storage_logs_chunks_duration: Family<StorageLogsChunksStage, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<SnapshotsApplierMetrics> = vise::Global::new();

#[derive(thiserror::Error, Debug)]
pub enum SnapshotsApplierError {
    #[error("canceled")]
    Canceled(String),
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
    #[error(transparent)]
    Retryable(anyhow::Error),
}

#[derive(Debug)]
pub struct SnapshotsApplier<'a, 'b> {
    connection_pool: &'a ConnectionPool,
    blob_store: &'b dyn ObjectStore,
    applied_snapshot_status: SnapshotRecoveryStatus,
}

impl From<ObjectStoreError> for SnapshotsApplierError {
    fn from(error: ObjectStoreError) -> Self {
        match error {
            ObjectStoreError::KeyNotFound(_) | ObjectStoreError::Serialization(_) => {
                Self::Fatal(error.into())
            }
            ObjectStoreError::Other(_) => Self::Retryable(error.into()),
        }
    }
}

impl From<SqlxError> for SnapshotsApplierError {
    fn from(error: SqlxError) -> Self {
        match error {
            SqlxError::Database(_)
            | SqlxError::RowNotFound
            | SqlxError::ColumnNotFound(_)
            | SqlxError::Configuration(_)
            | SqlxError::TypeNotFound { .. } => Self::Fatal(error.into()),
            _ => Self::Retryable(error.into()),
        }
    }
}

impl From<RpcError> for SnapshotsApplierError {
    fn from(error: RpcError) -> Self {
        match error {
            Error::Transport(_) | Error::RequestTimeout | Error::RestartNeeded(_) => {
                Self::Retryable(error.into())
            }
            _ => Self::Fatal(error.into()),
        }
    }
}

#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn fetch_l2_block(&self, number: MiniblockNumber) -> Result<Option<SyncBlock>, RpcError>;

    async fn fetch_newest_snapshot(&self) -> Result<Option<SnapshotHeader>, RpcError>;
}
impl<'a, 'b> SnapshotsApplier<'a, 'b> {
    pub async fn prepare_applied_snapshot_status(
        storage: &mut StorageProcessor<'_>,
        main_node_client: &'_ dyn SnapshotsApplierMainNodeClient,
    ) -> Result<(SnapshotRecoveryStatus, bool), SnapshotsApplierError> {
        let latency =
            METRICS.initial_stage_duration[&InitialStage::FetchMetadataFromMainNode].start();

        let applied_snapshot_status = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?;

        if let Some(applied_snapshot_status) = applied_snapshot_status {
            if !applied_snapshot_status
                .storage_logs_chunks_processed
                .contains(&false)
            {
                return Err(SnapshotsApplierError::Canceled(
                    "This node has already been initialized from a snapshot".to_string(),
                ));
            }

            let latency = latency.observe();
            tracing::info!("Re-initialized snapshots applier after reset/failure in {latency:?}");

            Ok((applied_snapshot_status, false))
        } else {
            if !storage.blocks_dal().is_genesis_needed().await? {
                return Err(SnapshotsApplierError::Canceled(
                    "This node has already been initialized without a snapshot".to_string(),
                ));
            }

            let latency = latency.observe();

            tracing::info!("Initialized fresh snapshots applier in {latency:?}");

            Ok((
                SnapshotsApplier::create_fresh_recovery_status(main_node_client).await?,
                true,
            ))
        }
    }

    pub async fn load_snapshot(
        connection_pool: &'a ConnectionPool,
        main_node_client: &'_ dyn SnapshotsApplierMainNodeClient,
        blob_store: &'b dyn ObjectStore,
    ) -> Result<(), SnapshotsApplierError> {
        let mut storage = connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage = storage.start_transaction().await?;

        let (applied_snapshot_status, created_from_scratch) =
            SnapshotsApplier::prepare_applied_snapshot_status(&mut storage, main_node_client)
                .await?;

        let mut recovery = Self {
            connection_pool,
            blob_store,
            applied_snapshot_status,
        };

        METRICS.storage_logs_chunks_count.set(
            recovery
                .applied_snapshot_status
                .storage_logs_chunks_processed
                .len(),
        );

        METRICS.storage_logs_chunks_left_to_process.set(
            recovery
                .applied_snapshot_status
                .storage_logs_chunks_processed
                .iter()
                .filter(|x| !(**x))
                .count(),
        );

        if created_from_scratch {
            recovery.recover_factory_deps(&mut storage).await?;

            storage
                .snapshot_recovery_dal()
                .insert_initial_recovery_status(&recovery.applied_snapshot_status)
                .await?;
        }

        storage.commit().await?;

        recovery.recover_storage_logs().await?;

        Ok(())
    }

    async fn create_fresh_recovery_status(
        main_node_client: &'_ dyn SnapshotsApplierMainNodeClient,
    ) -> Result<SnapshotRecoveryStatus, SnapshotsApplierError> {
        let snapshot_response = main_node_client.fetch_newest_snapshot().await?;

        let snapshot = snapshot_response.ok_or(SnapshotsApplierError::Canceled(
            "Main node does not have any ready snapshots, skipping initialization from snapshot!"
                .to_string(),
        ))?;

        let l1_batch_number = snapshot.l1_batch_number;
        tracing::info!(
            "Found snapshot with data up to l1_batch {}, storage_logs are divided into {} chunk(s)",
            l1_batch_number,
            snapshot.storage_logs_chunks.len()
        );

        let miniblock = main_node_client
            .fetch_l2_block(snapshot.miniblock_number)
            .await?
            .ok_or(SnapshotsApplierError::Fatal(anyhow::anyhow!(
                "Miniblock {} is missing",
                snapshot.miniblock_number
            )))?;
        let miniblock_root_hash = miniblock.hash.unwrap();

        Ok(SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_root_hash: snapshot.last_l1_batch_with_metadata.metadata.root_hash,
            miniblock_number: snapshot.miniblock_number,
            miniblock_root_hash,
            storage_logs_chunks_processed: vec![false; snapshot.storage_logs_chunks.len()],
        })
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        let latency = METRICS.initial_stage_duration[&InitialStage::ApplyFactoryDeps].start();

        let factory_deps: SnapshotFactoryDependencies = self
            .blob_store
            .get(self.applied_snapshot_status.l1_batch_number)
            .await?;

        let all_deps_hashmap: HashMap<H256, Vec<u8>> = factory_deps
            .factory_deps
            .into_iter()
            .map(|dep| (hash_bytecode(&dep.bytecode.0), dep.bytecode.0))
            .collect();
        storage
            .storage_dal()
            .insert_factory_deps(
                self.applied_snapshot_status.miniblock_number,
                &all_deps_hashmap,
            )
            .await?;

        let latency = latency.observe();
        tracing::info!("Applied factory dependencies in {latency:?}");

        Ok(())
    }

    async fn insert_initial_writes_chunk(
        &mut self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dedup_dal()
            .insert_initial_writes_from_snapshot(storage_logs)
            .await?;
        Ok(())
    }
    async fn insert_storage_logs_chunk(
        &mut self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dal()
            .insert_storage_logs_from_snapshot(
                self.applied_snapshot_status.miniblock_number,
                storage_logs,
            )
            .await?;
        Ok(())
    }

    async fn recover_storage_logs_single_chunk(
        &mut self,
        chunk_id: u64,
    ) -> Result<(), SnapshotsApplierError> {
        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::LoadFromGcs].start();

        let storage_key = SnapshotStorageLogsStorageKey {
            chunk_id,
            l1_batch_number: self.applied_snapshot_status.l1_batch_number,
        };

        tracing::info!("Processing chunk {chunk_id}");

        let storage_snapshot_chunk: SnapshotStorageLogsChunk =
            self.blob_store.get(storage_key).await?;

        let latency = latency.observe();
        tracing::info!("Loaded storage logs from GCS for chunk {chunk_id} in {latency:?}");

        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::SaveToPostgres].start();

        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage = storage.start_transaction().await?;

        let storage_logs = &storage_snapshot_chunk.storage_logs;

        tracing::info!("Loading {} storage logs into postgres", storage_logs.len());

        self.insert_storage_logs_chunk(storage_logs, &mut storage)
            .await?;

        self.insert_initial_writes_chunk(storage_logs, &mut storage)
            .await?;

        self.applied_snapshot_status.storage_logs_chunks_processed[chunk_id as usize] = true;
        storage
            .snapshot_recovery_dal()
            .mark_storage_logs_chunk_as_processed(chunk_id)
            .await?;

        storage.commit().await?;

        let chunks_left = METRICS.storage_logs_chunks_left_to_process.dec_by(1) - 1;

        let latency = latency.observe();
        tracing::info!("Saved storage logs for chunk {chunk_id} in {latency:?}, there are {chunks_left} left to process");

        Ok(())
    }

    pub async fn recover_storage_logs(mut self) -> Result<(), SnapshotsApplierError> {
        for chunk_id in 0..self
            .applied_snapshot_status
            .storage_logs_chunks_processed
            .len()
        {
            //TODO Add retries and parallelize this step
            if !self.applied_snapshot_status.storage_logs_chunks_processed[chunk_id] {
                self.recover_storage_logs_single_chunk(chunk_id as u64)
                    .await?;
            }
        }

        Ok(())
    }
}

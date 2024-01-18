use std::{collections::HashMap, fmt, time::Duration};

use anyhow::Error;
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
use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

use crate::SnapshotApplierError::*;

mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum StorageChunkStage {
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
#[metrics(prefix = "snapshots_creator")]
pub(crate) struct SnapshotsCreatorMetrics {
    /// Number of chunks in the most recently generated snapshot. Set when a snapshot generation starts.
    pub storage_logs_chunks_count: Gauge<u64>,
    /// Number of chunks left to process for the snapshot being currently generated.
    pub storage_logs_chunks_left_to_process: Gauge<usize>,
    /// Total latency of applying snapshot.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub snapshot_applying_duration: Histogram<Duration>,

    /// Latency of storage log chunk processing split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub initial_stage_duration: Family<StorageChunkStage, Histogram<Duration>>,
}

#[derive(thiserror::Error, Debug)]
pub enum SnapshotApplierError {
    #[error("canceled")]
    Canceled(String),
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
    #[error("retryable")]
    Retryable(String),
}
pub struct SnapshotsApplier<'a> {
    connection_pool: &'a ConnectionPool,
    blob_store: Box<dyn ObjectStore>,
    applied_snapshot_status: SnapshotRecoveryStatus,
}

impl From<ObjectStoreError> for SnapshotApplierError {
    fn from(error: ObjectStoreError) -> Self {
        match error {
            ObjectStoreError::KeyNotFound(err) => {
                Fatal(Error::msg(format!("Key not found in object store: {err}")))
            }
            ObjectStoreError::Serialization(err) => {
                Fatal(Error::msg(format!("Unable to deserialize object: {err}")))
            }
            ObjectStoreError::Other(_) => {
                Retryable("An error occurred while accessing object store".to_string())
            }
        }
    }
}

impl From<SqlxError> for SnapshotApplierError {
    fn from(err: SqlxError) -> Self {
        match err {
            SqlxError::Database(_)
            | SqlxError::RowNotFound
            | SqlxError::ColumnNotFound(_)
            | SqlxError::Configuration(_)
            | SqlxError::TypeNotFound { type_name: _ } => Fatal(err.into()),
            _ => Retryable(format!("An error occured when accessing DB: {err}")),
        }
    }
}

#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn fetch_l2_block(&self, number: MiniblockNumber) -> Result<Option<SyncBlock>, RpcError>;

    async fn fetch_newest_snapshot(&self) -> Result<Option<SnapshotHeader>, RpcError>;
}
impl<'a> SnapshotsApplier<'a> {
    pub async fn load_snapshot(
        connection_pool: &'a ConnectionPool,
        main_node_client: Box<dyn SnapshotsApplierMainNodeClient>,
        blob_store: Box<dyn ObjectStore>,
    ) -> anyhow::Result<(), SnapshotApplierError> {
        let mut storage = connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage = storage.start_transaction().await?;

        let mut applied_snapshot_status = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .map_err(|_| Retryable("Unable to get applied snapshots status".to_string()))?;

        if !storage
            .blocks_dal()
            .is_genesis_needed()
            .await
            .map_err(|_| Retryable("Unable to fetch genesis state".to_string()))?
            && applied_snapshot_status.is_none()
        {
            return Err(Canceled(
                "This node has already been initialized without a snapshot".to_string(),
            ));
        }

        if let Some(applied_snapshot_status) = applied_snapshot_status.as_ref() {
            if !applied_snapshot_status
                .storage_logs_chunks_processed
                .contains(&false)
            {
                return Err(Canceled(
                    "This node has already been initialized from a snapshot".to_string(),
                ));
            }
        } else {
            applied_snapshot_status =
                Some(SnapshotsApplier::create_fresh_recovery_status(main_node_client).await?)
        }

        let mut recovery = Self {
            connection_pool,
            blob_store,
            applied_snapshot_status: applied_snapshot_status.unwrap(),
        };

        recovery.recover_factory_deps(&mut storage).await?;

        storage
            .snapshot_recovery_dal()
            .insert_initial_recovery_status(&recovery.applied_snapshot_status)
            .await
            .map_err(|err| Fatal(err.into()))?;

        storage.commit().await.map_err(|err| Fatal(err.into()))?;

        recovery.recover_storage_logs().await?;

        Ok(())
    }

    async fn create_fresh_recovery_status(
        main_node_client: Box<dyn SnapshotsApplierMainNodeClient>,
    ) -> Result<SnapshotRecoveryStatus, SnapshotApplierError> {
        let snapshot_response = main_node_client
            .fetch_newest_snapshot()
            .await
            .map_err(|_| Retryable("Unable to fetch newest snapshot from main node".to_string()))?;
        if snapshot_response.is_none() {
            return Err(Canceled("Main node does not have any ready snapshots, skipping initialization from snapshot!".to_string()));
        }
        let snapshot = snapshot_response.unwrap();
        let l1_batch_number = snapshot.l1_batch_number;
        tracing::info!(
            "Found snapshot with data up to l1_batch {}, storage_logs are divided into {} chunk(s)",
            l1_batch_number,
            snapshot.storage_logs_chunks.len()
        );

        let miniblock = main_node_client
            .fetch_l2_block(snapshot.miniblock_number)
            .await
            .map_err(|err| Fatal(err.into()))?
            .ok_or_else(|| Fatal(Error::msg("Miniblock is missing")))?;
        let miniblock_root_hash = miniblock.hash.unwrap();

        Ok(SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_root_hash: snapshot.last_l1_batch_with_metadata.metadata.root_hash,
            miniblock_number: snapshot.miniblock_number,
            miniblock_root_hash,
            storage_logs_chunks_processed: snapshot
                .storage_logs_chunks
                .iter()
                .map(|_| false)
                .collect(),
        })
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<(), SnapshotApplierError> {
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
        Ok(())
    }

    async fn insert_initial_writes_chunk(
        &mut self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotApplierError> {
        tracing::info!("Loading {} storage logs into postgres", storage_logs.len());
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
    ) -> Result<(), SnapshotApplierError> {
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
    ) -> Result<(), SnapshotApplierError> {
        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage = storage.start_transaction().await?;

        let storage_key = SnapshotStorageLogsStorageKey {
            chunk_id,
            l1_batch_number: self.applied_snapshot_status.l1_batch_number,
        };

        tracing::info!("Processing chunk {chunk_id}");

        let storage_snapshot_chunk: SnapshotStorageLogsChunk =
            self.blob_store.get(storage_key).await?;

        let storage_logs = &storage_snapshot_chunk.storage_logs;
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

        Ok(())
    }

    pub async fn recover_storage_logs(mut self) -> Result<(), SnapshotApplierError> {
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

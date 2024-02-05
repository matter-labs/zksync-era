//! Logic for applying application-level snapshots to Postgres storage.

use std::{collections::HashMap, fmt};

use anyhow::Context as _;
use async_trait::async_trait;
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

use self::metrics::{InitialStage, StorageLogsChunksStage, METRICS};

mod metrics;
#[cfg(test)]
mod tests;

#[derive(thiserror::Error, Debug)]
pub enum SnapshotsApplierError {
    #[error("canceled")]
    Canceled(String),
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
    #[error(transparent)]
    Retryable(anyhow::Error),
}

impl SnapshotsApplierError {
    fn canceled(message: &str) -> Self {
        Self::Canceled(message.to_owned())
    }
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

/// Main node API used by the [`SnapshotsApplier`].
#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn fetch_l2_block(&self, number: MiniblockNumber) -> Result<Option<SyncBlock>, RpcError>;

    async fn fetch_newest_snapshot(&self) -> Result<Option<SnapshotHeader>, RpcError>;
}

/// Applying application-level storage snapshots to the Postgres storage.
#[derive(Debug)]
pub struct SnapshotsApplier<'a> {
    connection_pool: &'a ConnectionPool,
    blob_store: &'a dyn ObjectStore,
    applied_snapshot_status: SnapshotRecoveryStatus,
}

impl<'a> SnapshotsApplier<'a> {
    /// Recovers [`SnapshotRecoveryStatus`] from the storage and the main node.
    async fn prepare_applied_snapshot_status(
        storage: &mut StorageProcessor<'_>,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
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
                return Err(SnapshotsApplierError::canceled(
                    "This node has already been initialized from a snapshot",
                ));
            }

            let latency = latency.observe();
            tracing::info!("Re-initialized snapshots applier after reset/failure in {latency:?}");

            Ok((applied_snapshot_status, false))
        } else {
            if !storage.blocks_dal().is_genesis_needed().await? {
                return Err(SnapshotsApplierError::canceled(
                    "This node has already been initialized without a snapshot",
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
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        blob_store: &'a dyn ObjectStore,
    ) -> Result<(), SnapshotsApplierError> {
        let mut storage = connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await?;

        let (applied_snapshot_status, created_from_scratch) =
            Self::prepare_applied_snapshot_status(&mut storage_transaction, main_node_client)
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
                .storage_logs_chunks_left_to_process(),
        );

        if created_from_scratch {
            recovery
                .recover_factory_deps(&mut storage_transaction)
                .await?;
            storage_transaction
                .snapshot_recovery_dal()
                .insert_initial_recovery_status(&recovery.applied_snapshot_status)
                .await?;
        }
        storage_transaction.commit().await?;
        drop(storage);

        recovery.recover_storage_logs().await?;
        Ok(())
    }

    async fn create_fresh_recovery_status(
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
    ) -> Result<SnapshotRecoveryStatus, SnapshotsApplierError> {
        let snapshot_response = main_node_client.fetch_newest_snapshot().await?;

        let snapshot = snapshot_response.ok_or(SnapshotsApplierError::canceled(
            "Main node does not have any ready snapshots, skipping initialization from snapshot!",
        ))?;

        let l1_batch_number = snapshot.l1_batch_number;
        let miniblock_number = snapshot.miniblock_number;
        tracing::info!(
            "Found snapshot with data up to L1 batch #{l1_batch_number}, storage_logs are divided into {} chunk(s)",
            snapshot.storage_logs_chunks.len()
        );

        let miniblock = main_node_client
            .fetch_l2_block(miniblock_number)
            .await?
            .with_context(|| format!("miniblock #{miniblock_number} is missing on main node"))?;
        let miniblock_root_hash = miniblock
            .hash
            .context("snapshot miniblock fetched from main node doesn't have hash set")?;

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

        tracing::debug!("Fetching factory dependencies from object store");
        let factory_deps: SnapshotFactoryDependencies = self
            .blob_store
            .get(self.applied_snapshot_status.l1_batch_number)
            .await?;
        tracing::debug!(
            "Fetched {} factory dependencies from object store",
            factory_deps.factory_deps.len()
        );

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

    #[tracing::instrument(level = "debug", err, skip(self))]
    async fn recover_storage_logs_single_chunk(
        &mut self,
        chunk_id: u64,
    ) -> Result<(), SnapshotsApplierError> {
        tracing::info!("Processing storage logs chunk {chunk_id}");
        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::LoadFromGcs].start();

        let storage_key = SnapshotStorageLogsStorageKey {
            chunk_id,
            l1_batch_number: self.applied_snapshot_status.l1_batch_number,
        };
        let storage_snapshot_chunk: SnapshotStorageLogsChunk =
            self.blob_store.get(storage_key).await?;
        let storage_logs = &storage_snapshot_chunk.storage_logs;
        let latency = latency.observe();
        tracing::info!(
            "Loaded {} storage logs from GCS for chunk {chunk_id} in {latency:?}",
            storage_logs.len()
        );

        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::SaveToPostgres].start();

        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await?;

        tracing::info!("Loading {} storage logs into Postgres", storage_logs.len());
        self.insert_storage_logs_chunk(storage_logs, &mut storage_transaction)
            .await?;
        self.insert_initial_writes_chunk(storage_logs, &mut storage_transaction)
            .await?;

        self.applied_snapshot_status.storage_logs_chunks_processed[chunk_id as usize] = true;
        storage_transaction
            .snapshot_recovery_dal()
            .mark_storage_logs_chunk_as_processed(chunk_id)
            .await?;
        storage_transaction.commit().await?;

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

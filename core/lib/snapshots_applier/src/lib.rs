use std::{collections::HashMap, fmt};

use anyhow::Error;
use async_trait::async_trait;

use zksync_core::sync_layer::MainNodeClient;
use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    zkevm_test_harness::k256::pkcs8::der::EncodeValue,
    MiniblockNumber, H256,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::{
    jsonrpsee::core::ClientError as RpcError,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::SnapshotApplierError::*;

mod tests;

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
            SqlxError::Database(_) => Fatal(err.into()),
            SqlxError::RowNotFound => Fatal(err.into()),
            SqlxError::ColumnNotFound(_) => Fatal(err.into()),
            SqlxError::Configuration(_) => Fatal(err.into()),
            SqlxError::TypeNotFound { type_name: _ } => Fatal(err.into()),
            _ => Retryable(format!("An error occured when accessing DB: {err}")),
        }
    }
}

#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn miniblock_hash(&self, number: MiniblockNumber) -> Result<Option<H256>, RpcError>;

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
            if applied_snapshot_status
                .storage_logs_chunks_ids_to_process
                .is_empty()
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
            .set_applied_snapshot_status(&recovery.applied_snapshot_status)
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

        let miniblock_root_hash = main_node_client
            .miniblock_hash(snapshot.miniblock_number)
            .await
            .map_err(|err| Fatal(err.into()))?
            .ok_or_else(|| Fatal(Error::msg("Miniblock is missing")))?;

        let chunk_ids = snapshot
            .storage_logs_chunks
            .iter()
            .map(|x| x.chunk_id)
            .collect();
        Ok(SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_root_hash: snapshot.last_l1_batch_with_metadata.metadata.root_hash,
            miniblock_number: snapshot.miniblock_number,
            miniblock_root_hash,
            storage_logs_chunks_ids_already_processed: vec![],
            storage_logs_chunks_ids_to_process: chunk_ids,
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

        self.applied_snapshot_status
            .storage_logs_chunks_ids_already_processed
            .push(chunk_id);
        let index_to_remove = self
            .applied_snapshot_status
            .storage_logs_chunks_ids_to_process
            .iter()
            .position(|x| *x == chunk_id)
            .unwrap();
        self.applied_snapshot_status
            .storage_logs_chunks_ids_to_process
            .remove(index_to_remove);
        storage
            .snapshot_recovery_dal()
            .set_applied_snapshot_status(&self.applied_snapshot_status)
            .await?;

        storage.commit().await?;

        Ok(())
    }

    pub async fn recover_storage_logs(mut self) -> Result<(), SnapshotApplierError> {
        for chunk_id in self
            .applied_snapshot_status
            .storage_logs_chunks_ids_to_process
            .clone()
            .iter()
        {
            //TODO Add retries and parallelize this step
            self.recover_storage_logs_single_chunk(*chunk_id).await?;
        }

        Ok(())
    }
}

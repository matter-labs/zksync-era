//! Logic for applying application-level snapshots to Postgres storage.

use std::{collections::HashMap, fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::Semaphore;
use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    api::en::SyncBlock,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    web3::futures,
    MiniblockNumber, H256,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::{
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::{core::client, http_client::HttpClient},
    namespaces::{EnNamespaceClient, SnapshotsNamespaceClient},
};

use self::metrics::{InitialStage, StorageLogsChunksStage, METRICS};

mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum SnapshotsApplierError {
    // Not really an error, just an early return from snapshot application logic.
    #[error("snapshot application returned early: {0:?}")]
    EarlyReturn(SnapshotsApplierOutcome),
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
    #[error(transparent)]
    Retryable(anyhow::Error),
}

impl SnapshotsApplierError {
    fn object_store(err: ObjectStoreError, context: String) -> Self {
        match err {
            ObjectStoreError::KeyNotFound(_) | ObjectStoreError::Serialization(_) => {
                Self::Fatal(anyhow::Error::from(err).context(context))
            }
            ObjectStoreError::Other(_) => {
                Self::Retryable(anyhow::Error::from(err).context(context))
            }
        }
    }

    fn db(err: SqlxError, context: impl Into<String>) -> Self {
        let context = context.into();
        match err {
            SqlxError::Database(_)
            | SqlxError::RowNotFound
            | SqlxError::ColumnNotFound(_)
            | SqlxError::Configuration(_)
            | SqlxError::TypeNotFound { .. } => {
                Self::Fatal(anyhow::Error::from(err).context(context))
            }
            _ => Self::Retryable(anyhow::Error::from(err).context(context)),
        }
    }
}

impl From<SnapshotsApplierOutcome> for SnapshotsApplierError {
    fn from(outcome: SnapshotsApplierOutcome) -> Self {
        Self::EarlyReturn(outcome)
    }
}

impl From<EnrichedClientError> for SnapshotsApplierError {
    fn from(err: EnrichedClientError) -> Self {
        match err.as_ref() {
            client::Error::Transport(_) | client::Error::RequestTimeout => {
                Self::Retryable(err.into())
            }
            _ => Self::Fatal(err.into()),
        }
    }
}

/// Non-erroneous outcomes of the snapshot application.
#[must_use = "Depending on app, `NoSnapshotsOnMainNode` may be considered an error"]
#[derive(Debug)]
pub enum SnapshotsApplierOutcome {
    /// The node DB was successfully recovered from a snapshot.
    Ok,
    /// Main node does not have any ready snapshots.
    NoSnapshotsOnMainNode,
    /// The node was initialized without a snapshot. Detected by the presence of the genesis L1 batch in Postgres.
    InitializedWithoutSnapshot,
}

/// Main node API used by the [`SnapshotsApplier`].
#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
    ) -> EnrichedClientResult<Option<SyncBlock>>;

    async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>>;
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for HttpClient {
    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
    ) -> EnrichedClientResult<Option<SyncBlock>> {
        self.sync_l2_block(number, false)
            .rpc_context("sync_l2_block")
            .with_arg("number", &number)
            .await
    }

    async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>> {
        let snapshots = self
            .get_all_snapshots()
            .rpc_context("get_all_snapshots")
            .await?;
        let Some(newest_snapshot) = snapshots.snapshots_l1_batch_numbers.first() else {
            return Ok(None);
        };
        self.get_snapshot_by_l1_batch_number(*newest_snapshot)
            .rpc_context("get_snapshot_by_l1_batch_number")
            .with_arg("number", newest_snapshot)
            .await
    }
}

/// Snapshot applier configuration options.
#[derive(Debug)]
pub struct SnapshotsApplierConfig {
    pub retry_count: usize,
    pub initial_retry_backoff: Duration,
    pub retry_backoff_multiplier: f32,
}

impl Default for SnapshotsApplierConfig {
    fn default() -> Self {
        Self {
            retry_count: 5,
            initial_retry_backoff: Duration::from_secs(2),
            retry_backoff_multiplier: 2.0,
        }
    }
}

impl SnapshotsApplierConfig {
    #[cfg(test)]
    fn for_tests() -> Self {
        Self {
            initial_retry_backoff: Duration::from_millis(5),
            ..Self::default()
        }
    }

    /// Runs the snapshot applier with these options.
    pub async fn run(
        self,
        connection_pool: &ConnectionPool,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        blob_store: &dyn ObjectStore,
    ) -> anyhow::Result<SnapshotsApplierOutcome> {
        let mut backoff = self.initial_retry_backoff;
        let mut last_error = None;
        for retry_id in 0..self.retry_count {
            let result =
                SnapshotsApplier::load_snapshot(connection_pool, main_node_client, blob_store)
                    .await;

            match result {
                Ok(()) => return Ok(SnapshotsApplierOutcome::Ok),
                Err(SnapshotsApplierError::Fatal(err)) => {
                    tracing::error!("Fatal error occurred during snapshots recovery: {err:?}");
                    return Err(err);
                }
                Err(SnapshotsApplierError::EarlyReturn(outcome)) => {
                    tracing::info!("Cancelled snapshots recovery with the outcome: {outcome:?}");
                    return Ok(outcome);
                }
                Err(SnapshotsApplierError::Retryable(err)) => {
                    tracing::warn!("Retryable error occurred during snapshots recovery: {err:?}");
                    last_error = Some(err);
                    tracing::info!(
                        "Recovering from error; attempt {retry_id} / {}, retrying in {backoff:?}",
                        self.retry_count
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.mul_f32(self.retry_backoff_multiplier);
                }
            }
        }

        let last_error = last_error.unwrap(); // `unwrap()` is safe: `last_error` was assigned at least once
        tracing::error!("Snapshot recovery run out of retries; last error: {last_error:?}");
        Err(last_error)
    }
}

/// Applying application-level storage snapshots to the Postgres storage.
#[derive(Debug)]
struct SnapshotsApplier<'a> {
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
            .await
            .map_err(|err| {
                SnapshotsApplierError::db(err, "failed fetching applied snapshot status from DB")
            })?;

        if let Some(applied_snapshot_status) = applied_snapshot_status {
            if !applied_snapshot_status
                .storage_logs_chunks_processed
                .contains(&false)
            {
                return Err(SnapshotsApplierOutcome::Ok.into());
            }

            let latency = latency.observe();
            tracing::info!("Re-initialized snapshots applier after reset/failure in {latency:?}");

            Ok((applied_snapshot_status, false))
        } else {
            let is_genesis_needed =
                storage
                    .blocks_dal()
                    .is_genesis_needed()
                    .await
                    .map_err(|err| {
                        SnapshotsApplierError::db(err, "failed checking genesis L1 batch in DB")
                    })?;
            if !is_genesis_needed {
                return Err(SnapshotsApplierOutcome::InitializedWithoutSnapshot.into());
            }

            let latency = latency.observe();
            tracing::info!("Initialized fresh snapshots applier in {latency:?}");
            Ok((
                SnapshotsApplier::create_fresh_recovery_status(main_node_client).await?,
                true,
            ))
        }
    }

    async fn load_snapshot(
        connection_pool: &'a ConnectionPool,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        blob_store: &'a dyn ObjectStore,
    ) -> Result<(), SnapshotsApplierError> {
        let mut storage = connection_pool
            .access_storage_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await.map_err(|err| {
            SnapshotsApplierError::db(err, "failed starting initial DB transaction")
        })?;

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
                .await
                .map_err(|err| {
                    SnapshotsApplierError::db(err, "failed persisting initial recovery status")
                })?;
        }
        storage_transaction.commit().await.map_err(|err| {
            SnapshotsApplierError::db(err, "failed committing initial DB transaction")
        })?;
        drop(storage);

        recovery.recover_storage_logs().await?;
        Ok(())
    }

    async fn create_fresh_recovery_status(
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
    ) -> Result<SnapshotRecoveryStatus, SnapshotsApplierError> {
        let snapshot_response = main_node_client.fetch_newest_snapshot().await?;

        let snapshot = snapshot_response.ok_or(SnapshotsApplierOutcome::NoSnapshotsOnMainNode)?;
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
        let miniblock_hash = miniblock
            .hash
            .context("snapshot miniblock fetched from main node doesn't have hash set")?;

        Ok(SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_timestamp: snapshot.last_l1_batch_with_metadata.header.timestamp,
            l1_batch_root_hash: snapshot.last_l1_batch_with_metadata.metadata.root_hash,
            miniblock_number: snapshot.miniblock_number,
            miniblock_timestamp: miniblock.timestamp,
            miniblock_hash,
            protocol_version: snapshot
                .last_l1_batch_with_metadata
                .header
                .protocol_version
                .unwrap(),
            storage_logs_chunks_processed: vec![false; snapshot.storage_logs_chunks.len()],
        })
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        let latency = METRICS.initial_stage_duration[&InitialStage::ApplyFactoryDeps].start();

        tracing::debug!("Fetching factory dependencies from object store");
        let l1_batch_number = self.applied_snapshot_status.l1_batch_number;
        let factory_deps: SnapshotFactoryDependencies =
            self.blob_store.get(l1_batch_number).await.map_err(|err| {
                let context = format!(
                    "cannot fetch factory deps for L1 batch #{l1_batch_number} from object store"
                );
                SnapshotsApplierError::object_store(err, context)
            })?;
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
            .factory_deps_dal()
            .insert_factory_deps(
                self.applied_snapshot_status.miniblock_number,
                &all_deps_hashmap,
            )
            .await
            .map_err(|err| {
                SnapshotsApplierError::db(err, "failed persisting factory deps to DB")
            })?;

        let latency = latency.observe();
        tracing::info!("Applied factory dependencies in {latency:?}");

        Ok(())
    }

    async fn insert_initial_writes_chunk(
        &self,
        chunk_id: u64,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dedup_dal()
            .insert_initial_writes_from_snapshot(storage_logs)
            .await
            .map_err(|err| {
                let context =
                    format!("failed persisting initial writes from storage logs chunk {chunk_id}");
                SnapshotsApplierError::db(err, context)
            })?;
        Ok(())
    }

    async fn insert_storage_logs_chunk(
        &self,
        chunk_id: u64,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dal()
            .insert_storage_logs_from_snapshot(
                self.applied_snapshot_status.miniblock_number,
                storage_logs,
            )
            .await
            .map_err(|err| {
                let context = format!("failed persisting storage logs from chunk {chunk_id}");
                SnapshotsApplierError::db(err, context)
            })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", err, skip(self))]
    async fn recover_storage_logs_single_chunk(
        &self,
        semaphore: &Semaphore,
        chunk_id: u64,
    ) -> Result<(), SnapshotsApplierError> {
        // `unwrap()` is safe: the semaphore is never closed
        let _permit = semaphore.acquire().await.unwrap();

        tracing::info!("Processing storage logs chunk {chunk_id}");
        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::LoadFromGcs].start();

        let storage_key = SnapshotStorageLogsStorageKey {
            chunk_id,
            l1_batch_number: self.applied_snapshot_status.l1_batch_number,
        };
        let storage_snapshot_chunk: SnapshotStorageLogsChunk =
            self.blob_store.get(storage_key).await.map_err(|err| {
                let context =
                    format!("cannot fetch storage logs {storage_key:?} from object store");
                SnapshotsApplierError::object_store(err, context)
            })?;
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
        let mut storage_transaction = storage.start_transaction().await.map_err(|err| {
            let context = format!("cannot start DB transaction for storage logs chunk {chunk_id}");
            SnapshotsApplierError::db(err, context)
        })?;

        tracing::info!("Loading {} storage logs into Postgres", storage_logs.len());
        self.insert_storage_logs_chunk(chunk_id, storage_logs, &mut storage_transaction)
            .await?;
        self.insert_initial_writes_chunk(chunk_id, storage_logs, &mut storage_transaction)
            .await?;

        storage_transaction
            .snapshot_recovery_dal()
            .mark_storage_logs_chunk_as_processed(chunk_id)
            .await
            .map_err(|err| {
                let context = format!("failed marking storage logs chunk {chunk_id} as processed");
                SnapshotsApplierError::db(err, context)
            })?;
        storage_transaction.commit().await.map_err(|err| {
            let context = format!("cannot commit DB transaction for storage logs chunk {chunk_id}");
            SnapshotsApplierError::db(err, context)
        })?;

        let chunks_left = METRICS.storage_logs_chunks_left_to_process.dec_by(1) - 1;
        let latency = latency.observe();
        tracing::info!("Saved storage logs for chunk {chunk_id} in {latency:?}, there are {chunks_left} left to process");

        Ok(())
    }

    async fn recover_storage_logs(self) -> Result<(), SnapshotsApplierError> {
        let semaphore = Semaphore::new(self.connection_pool.max_size() as usize);
        let tasks = self
            .applied_snapshot_status
            .storage_logs_chunks_processed
            .iter()
            .enumerate()
            .filter(|(_, is_processed)| !**is_processed)
            .map(|(chunk_id, _)| {
                self.recover_storage_logs_single_chunk(&semaphore, chunk_id as u64)
            });
        futures::future::try_join_all(tasks).await?;

        Ok(())
    }
}

//! Logic for applying application-level snapshots to Postgres storage.

use std::{collections::HashMap, fmt, num::NonZeroUsize, sync::Arc, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Semaphore;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError, SqlxError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    api,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey, SnapshotVersion,
    },
    tokens::TokenInfo,
    L1BatchNumber, L2BlockNumber, H256,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::core::client,
    namespaces::{EnNamespaceClient, SnapshotsNamespaceClient, ZksNamespaceClient},
};

use self::metrics::{InitialStage, StorageLogsChunksStage, METRICS};

mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
struct SnapshotsApplierHealthDetails {
    snapshot_l2_block: L2BlockNumber,
    snapshot_l1_batch: L1BatchNumber,
    factory_deps_recovered: bool,
    storage_logs_chunk_count: usize,
    storage_logs_chunks_left_to_process: usize,
    tokens_recovered: bool,
}

impl SnapshotsApplierHealthDetails {
    fn done(status: &SnapshotRecoveryStatus) -> anyhow::Result<Self> {
        if status.storage_logs_chunks_left_to_process() != 0 {
            anyhow::bail!(
                "Inconsistent Postgres state: there are L2 blocks, but the snapshot recovery status \
                 contains unprocessed storage log chunks: {status:?}"
            );
        }

        Ok(Self {
            snapshot_l2_block: status.l2_block_number,
            snapshot_l1_batch: status.l1_batch_number,
            factory_deps_recovered: true,
            storage_logs_chunk_count: status.storage_logs_chunks_processed.len(),
            storage_logs_chunks_left_to_process: 0,
            tokens_recovered: true,
        })
    }

    fn is_done(&self) -> bool {
        self.factory_deps_recovered
            && self.tokens_recovered
            && self.storage_logs_chunks_left_to_process == 0
    }
}

#[derive(Debug, thiserror::Error)]
enum SnapshotsApplierError {
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
}

impl From<DalError> for SnapshotsApplierError {
    fn from(err: DalError) -> Self {
        match err.inner() {
            SqlxError::Database(_)
            | SqlxError::RowNotFound
            | SqlxError::ColumnNotFound(_)
            | SqlxError::Configuration(_)
            | SqlxError::TypeNotFound { .. } => Self::Fatal(anyhow::Error::from(err)),
            _ => Self::Retryable(anyhow::Error::from(err)),
        }
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

/// Main node API used by the [`SnapshotsApplier`].
#[async_trait]
pub trait SnapshotsApplierMainNodeClient: fmt::Debug + Send + Sync {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>>;

    async fn fetch_l2_block_details(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<api::BlockDetails>>;

    async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>>;

    async fn fetch_tokens(
        &self,
        at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>>;
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for Box<DynClient<L2>> {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>> {
        self.get_l1_batch_details(number)
            .rpc_context("get_l1_batch_details")
            .with_arg("number", &number)
            .await
    }

    async fn fetch_l2_block_details(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<api::BlockDetails>> {
        self.get_block_details(number)
            .rpc_context("get_block_details")
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

    async fn fetch_tokens(
        &self,
        at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        self.sync_tokens(Some(at_l2_block))
            .rpc_context("sync_tokens")
            .with_arg("l2_block_number", &at_l2_block)
            .await
    }
}

/// Snapshot applier configuration options.
#[derive(Debug)]
pub struct SnapshotsApplierConfig {
    /// Number of retries for transient errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    pub retry_count: usize,
    /// Initial back-off interval when retrying recovery on a transient error. Each subsequent retry interval
    /// will be multiplied by [`Self.retry_backoff_multiplier`].
    pub initial_retry_backoff: Duration,
    pub retry_backoff_multiplier: f32,
    /// Maximum concurrency factor when performing concurrent operations (for now, the only such operation
    /// is recovering chunks of storage logs).
    pub max_concurrency: NonZeroUsize,
}

impl Default for SnapshotsApplierConfig {
    fn default() -> Self {
        Self {
            retry_count: 5,
            initial_retry_backoff: Duration::from_secs(2),
            retry_backoff_multiplier: 2.0,
            max_concurrency: NonZeroUsize::new(10).unwrap(),
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
}

/// Stats returned by [`SnapshotsApplierTask::run()`].
#[derive(Debug)]
pub struct SnapshotApplierTaskStats {
    /// Did the task do any work?
    pub done_work: bool,
}

#[derive(Debug)]
pub struct SnapshotsApplierTask {
    config: SnapshotsApplierConfig,
    health_updater: HealthUpdater,
    connection_pool: ConnectionPool<Core>,
    main_node_client: Box<dyn SnapshotsApplierMainNodeClient>,
    blob_store: Arc<dyn ObjectStore>,
}

impl SnapshotsApplierTask {
    pub fn new(
        config: SnapshotsApplierConfig,
        connection_pool: ConnectionPool<Core>,
        main_node_client: Box<dyn SnapshotsApplierMainNodeClient>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> Self {
        Self {
            config,
            health_updater: ReactiveHealthCheck::new("snapshot_recovery").1,
            connection_pool,
            main_node_client,
            blob_store,
        }
    }

    /// Returns the health check for snapshot recovery.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    /// Runs the snapshot applier with these options.
    ///
    /// # Errors
    ///
    /// This method will return an error if a fatal error occurs during recovery (e.g., the DB is unreachable),
    /// or under any of the following conditions:
    ///
    /// - There are no snapshots on the main node
    pub async fn run(self) -> anyhow::Result<SnapshotApplierTaskStats> {
        tracing::info!("Starting snapshot recovery with config: {:?}", self.config);

        let mut backoff = self.config.initial_retry_backoff;
        let mut last_error = None;
        for retry_id in 0..self.config.retry_count {
            let result = SnapshotsApplier::load_snapshot(
                &self.connection_pool,
                self.main_node_client.as_ref(),
                &self.blob_store,
                &self.health_updater,
                self.config.max_concurrency.get(),
            )
            .await;

            match result {
                Ok((strategy, final_status)) => {
                    let health_details = SnapshotsApplierHealthDetails::done(&final_status)?;
                    self.health_updater
                        .update(Health::from(HealthStatus::Ready).with_details(health_details));
                    // Freeze the health check in the "ready" status, so that the snapshot recovery isn't marked
                    // as "shut down", which would lead to the app considered unhealthy.
                    self.health_updater.freeze();
                    return Ok(SnapshotApplierTaskStats {
                        done_work: !matches!(strategy, SnapshotRecoveryStrategy::Completed),
                    });
                }
                Err(SnapshotsApplierError::Fatal(err)) => {
                    tracing::error!("Fatal error occurred during snapshots recovery: {err:?}");
                    return Err(err);
                }
                Err(SnapshotsApplierError::Retryable(err)) => {
                    tracing::warn!("Retryable error occurred during snapshots recovery: {err:?}");
                    last_error = Some(err);
                    tracing::info!(
                        "Recovering from error; attempt {retry_id} / {}, retrying in {backoff:?}",
                        self.config.retry_count
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.mul_f32(self.config.retry_backoff_multiplier);
                }
            }
        }

        let last_error = last_error.unwrap(); // `unwrap()` is safe: `last_error` was assigned at least once
        tracing::error!("Snapshot recovery run out of retries; last error: {last_error:?}");
        Err(last_error)
    }
}

/// Strategy determining how snapshot recovery should proceed.
#[derive(Debug, Clone, Copy)]
enum SnapshotRecoveryStrategy {
    /// Snapshot recovery should proceed from scratch with the specified params.
    New,
    /// Snapshot recovery should continue with the specified params.
    Resumed,
    /// Snapshot recovery has already been completed.
    Completed,
}

impl SnapshotRecoveryStrategy {
    async fn new(
        storage: &mut Connection<'_, Core>,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
    ) -> Result<(Self, SnapshotRecoveryStatus), SnapshotsApplierError> {
        let latency =
            METRICS.initial_stage_duration[&InitialStage::FetchMetadataFromMainNode].start();
        let applied_snapshot_status = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?;

        if let Some(applied_snapshot_status) = applied_snapshot_status {
            let sealed_l2_block_number = storage.blocks_dal().get_sealed_l2_block_number().await?;
            if sealed_l2_block_number.is_some() {
                return Ok((Self::Completed, applied_snapshot_status));
            }

            let latency = latency.observe();
            tracing::info!("Re-initialized snapshots applier after reset/failure in {latency:?}");
            Ok((Self::Resumed, applied_snapshot_status))
        } else {
            let is_genesis_needed = storage.blocks_dal().is_genesis_needed().await?;
            if !is_genesis_needed {
                let err = anyhow::anyhow!(
                    "node contains a non-genesis L1 batch; snapshot recovery is unsafe"
                );
                return Err(SnapshotsApplierError::Fatal(err));
            }

            let recovery_status = Self::create_fresh_recovery_status(main_node_client).await?;

            let storage_logs_count = storage
                .storage_logs_dal()
                .get_storage_logs_row_count(recovery_status.l2_block_number)
                .await?;
            if storage_logs_count > 0 {
                let err = anyhow::anyhow!(
                    "storage_logs table has {storage_logs_count} rows at or before the snapshot L2 block #{}; \
                     snapshot recovery is unsafe",
                    recovery_status.l2_block_number
                );
                return Err(SnapshotsApplierError::Fatal(err));
            }

            let latency = latency.observe();
            tracing::info!("Initialized fresh snapshots applier in {latency:?}");
            Ok((Self::New, recovery_status))
        }
    }

    async fn create_fresh_recovery_status(
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
    ) -> Result<SnapshotRecoveryStatus, SnapshotsApplierError> {
        let snapshot_response = main_node_client.fetch_newest_snapshot().await?;

        let snapshot = snapshot_response
            .context("no snapshots on main node; snapshot recovery is impossible")?;
        let l1_batch_number = snapshot.l1_batch_number;
        let l2_block_number = snapshot.l2_block_number;
        tracing::info!(
            "Found snapshot with data up to L1 batch #{l1_batch_number}, L2 block #{l2_block_number}, \
            version {version}, storage logs are divided into {chunk_count} chunk(s)",
            version = snapshot.version,
            chunk_count = snapshot.storage_logs_chunks.len()
        );
        Self::check_snapshot_version(snapshot.version)?;

        let l1_batch = main_node_client
            .fetch_l1_batch_details(l1_batch_number)
            .await?
            .with_context(|| format!("L1 batch #{l1_batch_number} is missing on main node"))?;
        let l1_batch_root_hash = l1_batch
            .base
            .root_hash
            .context("snapshot L1 batch fetched from main node doesn't have root hash set")?;
        let l2_block = main_node_client
            .fetch_l2_block_details(l2_block_number)
            .await?
            .with_context(|| format!("L2 block #{l2_block_number} is missing on main node"))?;
        let l2_block_hash = l2_block
            .base
            .root_hash
            .context("snapshot L2 block fetched from main node doesn't have hash set")?;
        let protocol_version = l2_block.protocol_version.context(
            "snapshot L2 block fetched from main node doesn't have protocol version set",
        )?;
        if l2_block.l1_batch_number != l1_batch_number {
            let err = anyhow::anyhow!(
                "snapshot L2 block returned by main node doesn't belong to expected L1 batch #{l1_batch_number}: {l2_block:?}"
            );
            return Err(err.into());
        }

        Ok(SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_timestamp: l1_batch.base.timestamp,
            l1_batch_root_hash,
            l2_block_number: snapshot.l2_block_number,
            l2_block_timestamp: l2_block.base.timestamp,
            l2_block_hash,
            protocol_version,
            storage_logs_chunks_processed: vec![false; snapshot.storage_logs_chunks.len()],
        })
    }

    fn check_snapshot_version(raw_version: u16) -> anyhow::Result<()> {
        let version = SnapshotVersion::try_from(raw_version).with_context(|| {
            format!(
                "Unrecognized snapshot version: {raw_version}; make sure you're running the latest version of the node"
            )
        })?;
        anyhow::ensure!(
            matches!(version, SnapshotVersion::Version0),
            "Cannot recover from a snapshot with version {version:?}; the only supported version is {:?}",
            SnapshotVersion::Version0
        );
        Ok(())
    }
}

/// Applying application-level storage snapshots to the Postgres storage.
#[derive(Debug)]
struct SnapshotsApplier<'a> {
    connection_pool: &'a ConnectionPool<Core>,
    main_node_client: &'a dyn SnapshotsApplierMainNodeClient,
    blob_store: &'a dyn ObjectStore,
    applied_snapshot_status: SnapshotRecoveryStatus,
    health_updater: &'a HealthUpdater,
    max_concurrency: usize,
    factory_deps_recovered: bool,
    tokens_recovered: bool,
}

impl<'a> SnapshotsApplier<'a> {
    /// Returns final snapshot recovery status.
    async fn load_snapshot(
        connection_pool: &'a ConnectionPool<Core>,
        main_node_client: &'a dyn SnapshotsApplierMainNodeClient,
        blob_store: &'a dyn ObjectStore,
        health_updater: &'a HealthUpdater,
        max_concurrency: usize,
    ) -> Result<(SnapshotRecoveryStrategy, SnapshotRecoveryStatus), SnapshotsApplierError> {
        // While the recovery is in progress, the node is healthy (no error has occurred),
        // but is affected (its usual APIs don't work).
        health_updater.update(HealthStatus::Affected.into());

        let mut storage = connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await?;

        let (strategy, applied_snapshot_status) =
            SnapshotRecoveryStrategy::new(&mut storage_transaction, main_node_client).await?;
        tracing::info!("Chosen snapshot recovery strategy: {strategy:?} with status: {applied_snapshot_status:?}");
        let created_from_scratch = match strategy {
            SnapshotRecoveryStrategy::Completed => return Ok((strategy, applied_snapshot_status)),
            SnapshotRecoveryStrategy::New => true,
            SnapshotRecoveryStrategy::Resumed => false,
        };

        let mut this = Self {
            connection_pool,
            main_node_client,
            blob_store,
            applied_snapshot_status,
            health_updater,
            max_concurrency,
            factory_deps_recovered: !created_from_scratch,
            tokens_recovered: false,
        };

        METRICS.storage_logs_chunks_count.set(
            this.applied_snapshot_status
                .storage_logs_chunks_processed
                .len(),
        );
        METRICS.storage_logs_chunks_left_to_process.set(
            this.applied_snapshot_status
                .storage_logs_chunks_left_to_process(),
        );
        this.update_health();

        if created_from_scratch {
            this.recover_factory_deps(&mut storage_transaction).await?;
            storage_transaction
                .snapshot_recovery_dal()
                .insert_initial_recovery_status(&this.applied_snapshot_status)
                .await?;

            // Insert artificial entries into the pruning log so that it's guaranteed to match the snapshot recovery metadata.
            // This allows to not deal with the corner cases when a node was recovered from a snapshot, but its pruning log is empty.
            storage_transaction
                .pruning_dal()
                .soft_prune_batches_range(
                    this.applied_snapshot_status.l1_batch_number,
                    this.applied_snapshot_status.l2_block_number,
                )
                .await?;
            storage_transaction
                .pruning_dal()
                .hard_prune_batches_range(
                    this.applied_snapshot_status.l1_batch_number,
                    this.applied_snapshot_status.l2_block_number,
                )
                .await?;
        }
        storage_transaction.commit().await?;
        drop(storage);
        this.factory_deps_recovered = true;
        this.update_health();

        this.recover_storage_logs().await?;
        for is_chunk_processed in &mut this.applied_snapshot_status.storage_logs_chunks_processed {
            *is_chunk_processed = true;
        }

        this.recover_tokens().await?;
        this.tokens_recovered = true;
        this.update_health();
        Ok((strategy, this.applied_snapshot_status))
    }

    fn update_health(&self) {
        let details = SnapshotsApplierHealthDetails {
            snapshot_l2_block: self.applied_snapshot_status.l2_block_number,
            snapshot_l1_batch: self.applied_snapshot_status.l1_batch_number,
            factory_deps_recovered: self.factory_deps_recovered,
            tokens_recovered: self.tokens_recovered,
            storage_logs_chunk_count: self
                .applied_snapshot_status
                .storage_logs_chunks_processed
                .len(),
            // We don't use `self.applied_snapshot_status` here because it's not updated during recovery
            storage_logs_chunks_left_to_process: METRICS.storage_logs_chunks_left_to_process.get(),
        };
        let status = if details.is_done() {
            HealthStatus::Ready
        } else {
            HealthStatus::Affected
        };
        self.health_updater
            .update(Health::from(status).with_details(details));
    }

    async fn recover_factory_deps(
        &mut self,
        storage: &mut Connection<'_, Core>,
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
                self.applied_snapshot_status.l2_block_number,
                &all_deps_hashmap,
            )
            .await?;

        let latency = latency.observe();
        tracing::info!("Applied factory dependencies in {latency:?}");

        Ok(())
    }

    async fn insert_initial_writes_chunk(
        &self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dedup_dal()
            .insert_initial_writes_from_snapshot(storage_logs)
            .await?;
        Ok(())
    }

    async fn insert_storage_logs_chunk(
        &self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), SnapshotsApplierError> {
        storage
            .storage_logs_dal()
            .insert_storage_logs_from_snapshot(
                self.applied_snapshot_status.l2_block_number,
                storage_logs,
            )
            .await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", err, skip(self, semaphore))]
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
        self.validate_storage_logs_chunk(storage_logs)?;
        let latency = latency.observe();
        tracing::info!(
            "Loaded {} storage logs from GCS for chunk {chunk_id} in {latency:?}",
            storage_logs.len()
        );

        let latency =
            METRICS.storage_logs_chunks_duration[&StorageLogsChunksStage::SaveToPostgres].start();

        let mut storage = self
            .connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await?;

        tracing::info!("Loading {} storage logs into Postgres", storage_logs.len());
        self.insert_storage_logs_chunk(storage_logs, &mut storage_transaction)
            .await?;
        self.insert_initial_writes_chunk(storage_logs, &mut storage_transaction)
            .await?;

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

    /// Performs basic sanity check for a storage logs chunk.
    fn validate_storage_logs_chunk(
        &self,
        storage_logs: &[SnapshotStorageLog],
    ) -> anyhow::Result<()> {
        for log in storage_logs {
            anyhow::ensure!(
                log.enumeration_index > 0,
                "invalid storage log with zero enumeration_index: {log:?}"
            );
            anyhow::ensure!(
                log.l1_batch_number_of_initial_write <= self.applied_snapshot_status.l1_batch_number,
                "invalid storage log with `l1_batch_number_of_initial_write` from the future: {log:?}"
            );
        }
        Ok(())
    }

    async fn recover_storage_logs(&self) -> Result<(), SnapshotsApplierError> {
        let effective_concurrency =
            (self.connection_pool.max_size() as usize).min(self.max_concurrency);
        tracing::info!(
            "Recovering storage log chunks with {effective_concurrency} max concurrency"
        );
        let semaphore = Semaphore::new(effective_concurrency);

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

        let mut storage = self
            .connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        // This DB query is slow, but this is fine for verification purposes.
        let total_log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(self.applied_snapshot_status.l2_block_number)
            .await?;
        tracing::info!(
            "Recovered {total_log_count} storage logs in total; checking overall consistency..."
        );

        let number_of_logs_by_enum_indices = storage
            .snapshots_creator_dal()
            .get_distinct_storage_logs_keys_count(self.applied_snapshot_status.l1_batch_number)
            .await?;
        if number_of_logs_by_enum_indices != total_log_count {
            let err = anyhow::anyhow!(
                "mismatch between the expected number of storage logs by enumeration indices ({number_of_logs_by_enum_indices}) \
                 and the actual number of logs in the snapshot ({total_log_count}); the snapshot may be corrupted"
            );
            return Err(SnapshotsApplierError::Fatal(err));
        }

        Ok(())
    }

    /// Needs to run after recovering storage logs.
    async fn recover_tokens(&self) -> Result<(), SnapshotsApplierError> {
        // Check whether tokens are already recovered.
        let mut storage = self
            .connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let all_token_addresses = storage.tokens_dal().get_all_l2_token_addresses().await?;
        if !all_token_addresses.is_empty() {
            tracing::info!(
                "{} tokens are already present in DB; skipping token recovery",
                all_token_addresses.len()
            );
            return Ok(());
        }
        drop(storage);

        let snapshot_l2_block_number = self.applied_snapshot_status.l2_block_number;
        let tokens = self
            .main_node_client
            .fetch_tokens(snapshot_l2_block_number)
            .await?;
        tracing::info!("Retrieved {} tokens from main node", tokens.len());

        // Check that all tokens returned by the main node were indeed successfully deployed.
        let l2_addresses = tokens.iter().map(|token| token.l2_address);
        let mut storage = self
            .connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let filtered_addresses = storage
            .storage_logs_dal()
            .filter_deployed_contracts(l2_addresses, Some(snapshot_l2_block_number))
            .await?;

        let bogus_tokens = tokens.iter().filter(|token| {
            // We need special handling for L2 ether; its `l2_address` doesn't have a deployed contract
            !token.l2_address.is_zero() && !filtered_addresses.contains_key(&token.l2_address)
        });
        let bogus_tokens: Vec<_> = bogus_tokens.collect();
        if !bogus_tokens.is_empty() {
            let err = anyhow::anyhow!(
                "Main node returned bogus tokens that are not deployed on L2: {bogus_tokens:?}"
            );
            return Err(SnapshotsApplierError::Retryable(err));
        }

        tracing::info!(
            "Checked {} tokens deployment on L2; persisting tokens into DB",
            tokens.len()
        );
        storage.tokens_dal().add_tokens(&tokens).await?;
        Ok(())
    }
}

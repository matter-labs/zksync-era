//! Logic for applying application-level snapshots to Postgres storage.

use std::{
    cmp::Ordering, collections::HashMap, fmt, mem, num::NonZeroUsize, sync::Arc, time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{watch, Semaphore};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError, SqlxError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    api,
    bytecode::{BytecodeHash, BytecodeMarker},
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey, SnapshotVersion,
    },
    tokens::TokenInfo,
    L1BatchNumber, L2BlockNumber, OrStopped, StorageKey, H256,
};
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
        if err.is_retriable() {
            Self::Retryable(anyhow::Error::from(err).context(context))
        } else {
            Self::Fatal(anyhow::Error::from(err).context(context))
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

type StopResult<T> = Result<T, OrStopped<SnapshotsApplierError>>;

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

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>>;

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>>;

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

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>> {
        let snapshots = self
            .get_all_snapshots()
            .rpc_context("get_all_snapshots")
            .await?;
        Ok(snapshots.snapshots_l1_batch_numbers.first().copied())
    }

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>> {
        self.get_snapshot_by_l1_batch_number(l1_batch_number)
            .rpc_context("get_snapshot_by_l1_batch_number")
            .with_arg("number", &l1_batch_number)
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

/// Reported status of the snapshot recovery progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecoveryCompletionStatus {
    /// There is no infomration about snapshot recovery in the database.
    NoRecoveryDetected,
    /// Snapshot recovery is not finished yet.
    InProgress,
    /// Snapshot recovery is completed.
    Completed,
}

/// Snapshot applier configuration options.
#[derive(Debug, Clone)]
pub struct SnapshotsApplierConfig {
    /// Number of retries for retriable errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    pub retry_count: usize,
    /// Initial back-off interval when retrying recovery on a retriable error. Each subsequent retry interval
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
    snapshot_l1_batch: Option<L1BatchNumber>,
    drop_storage_key_preimages: bool,
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
            snapshot_l1_batch: None,
            drop_storage_key_preimages: false,
            config,
            health_updater: ReactiveHealthCheck::new("snapshot_recovery").1,
            connection_pool,
            main_node_client,
            blob_store,
        }
    }

    /// Checks whether the snapshot recovery is already completed.
    ///
    /// Returns `None` if no snapshot recovery information is detected in the DB.
    /// Returns `Some(true)` if the recovery is completed.
    /// Returns `Some(false)` if the recovery is not completed.
    pub async fn is_recovery_completed(
        conn: &mut Connection<'_, Core>,
        client: &dyn SnapshotsApplierMainNodeClient,
    ) -> anyhow::Result<RecoveryCompletionStatus> {
        let Some(applied_snapshot_status) = conn
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?
        else {
            return Ok(RecoveryCompletionStatus::NoRecoveryDetected);
        };
        // If there are unprocessed storage logs chunks, the recovery is not complete.
        if applied_snapshot_status.storage_logs_chunks_left_to_process() != 0 {
            return Ok(RecoveryCompletionStatus::InProgress);
        }
        // Currently, migrating tokens is the last step of the recovery.
        // The number of tokens is not a part of the snapshot header, so we have to re-query the main node.
        let added_tokens = conn
            .tokens_web3_dal()
            .get_all_tokens(Some(applied_snapshot_status.l2_block_number))
            .await?
            .len();
        let tokens_on_main_node = client
            .fetch_tokens(applied_snapshot_status.l2_block_number)
            .await?
            .len();

        match added_tokens.cmp(&tokens_on_main_node) {
            Ordering::Less => Ok(RecoveryCompletionStatus::InProgress),
            Ordering::Equal => Ok(RecoveryCompletionStatus::Completed),
            Ordering::Greater => anyhow::bail!("DB contains more tokens than the main node"),
        }
    }

    /// Specifies the L1 batch to recover from. This setting is ignored if recovery is complete or resumed.
    pub fn set_snapshot_l1_batch(&mut self, number: L1BatchNumber) {
        self.snapshot_l1_batch = Some(number);
    }

    /// Enables dropping storage key preimages when recovering storage logs from a snapshot with version 0.
    /// This is a temporary flag that will eventually be removed together with version 0 snapshot support.
    pub fn drop_storage_key_preimages(&mut self) {
        self.drop_storage_key_preimages = true;
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
    pub async fn run(
        self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> Result<SnapshotApplierTaskStats, OrStopped> {
        tracing::info!("Starting snapshot recovery with config: {:?}", self.config);

        let mut backoff = self.config.initial_retry_backoff;
        let mut last_error = None;
        for retry_id in 0..self.config.retry_count {
            if *stop_receiver.borrow() {
                return Err(OrStopped::Stopped);
            }

            let result = SnapshotsApplierOrStatus::load_snapshot(&self, &mut stop_receiver).await;
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
                Err(OrStopped::Internal(SnapshotsApplierError::Fatal(err))) => {
                    tracing::error!("Fatal error occurred during snapshots recovery: {err:?}");
                    return Err(err.into());
                }
                Err(OrStopped::Internal(SnapshotsApplierError::Retryable(err))) => {
                    tracing::warn!("Retryable error occurred during snapshots recovery: {err:?}");
                    last_error = Some(err);
                    tracing::info!(
                        "Recovering from error; attempt {retry_id} / {}, retrying in {backoff:?}",
                        self.config.retry_count
                    );
                    tokio::time::timeout(backoff, stop_receiver.changed())
                        .await
                        .ok();
                    // Stop receiver will be checked on the next iteration.
                    backoff = backoff.mul_f32(self.config.retry_backoff_multiplier);
                }
                Err(OrStopped::Stopped) => {
                    tracing::info!("Snapshot recovery has been canceled");
                    return Err(OrStopped::Stopped);
                }
            }
        }

        let last_error = last_error.unwrap(); // `unwrap()` is safe: `last_error` was assigned at least once
        tracing::error!("Snapshot recovery run out of retries; last error: {last_error:?}");
        Err(last_error.into())
    }
}

/// Strategy determining how snapshot recovery should proceed.
#[derive(Debug, Clone, Copy)]
enum SnapshotRecoveryStrategy {
    /// Snapshot recovery should proceed from scratch with the specified params.
    New(SnapshotVersion),
    /// Snapshot recovery should continue with the specified params.
    Resumed(SnapshotVersion),
    /// Snapshot recovery has already been completed.
    Completed,
}

impl SnapshotRecoveryStrategy {
    async fn new(
        storage: &mut Connection<'_, Core>,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        snapshot_l1_batch: Option<L1BatchNumber>,
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

            let l1_batch_number = applied_snapshot_status.l1_batch_number;
            let snapshot_header = main_node_client
                .fetch_snapshot(l1_batch_number)
                .await?
                .with_context(|| {
                    format!("snapshot for L1 batch #{l1_batch_number} is no longer present on main node")
                })?;
            // Old snapshots can theoretically be removed by the node, but in this case the snapshot data may be removed as well,
            // so returning an error looks appropriate here.
            let snapshot_version = Self::check_snapshot_version(snapshot_header.version)?;

            let latency = latency.observe();
            tracing::info!("Re-initialized snapshots applier after reset/failure in {latency:?}");
            Ok((Self::Resumed(snapshot_version), applied_snapshot_status))
        } else {
            let is_genesis_needed = storage.blocks_dal().is_genesis_needed().await?;
            if !is_genesis_needed {
                let err = anyhow::anyhow!(
                    "node contains a non-genesis L1 batch; snapshot recovery is unsafe"
                );
                return Err(SnapshotsApplierError::Fatal(err));
            }

            let (recovery_status, snapshot_version) =
                Self::create_fresh_recovery_status(main_node_client, snapshot_l1_batch).await?;

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
            Ok((Self::New(snapshot_version), recovery_status))
        }
    }

    async fn create_fresh_recovery_status(
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        snapshot_l1_batch: Option<L1BatchNumber>,
    ) -> Result<(SnapshotRecoveryStatus, SnapshotVersion), SnapshotsApplierError> {
        let l1_batch_number = match snapshot_l1_batch {
            Some(num) => num,
            None => main_node_client
                .fetch_newest_snapshot_l1_batch_number()
                .await?
                .context("no snapshots on main node; snapshot recovery is impossible")?,
        };
        let snapshot_response = main_node_client.fetch_snapshot(l1_batch_number).await?;

        let snapshot = snapshot_response.with_context(|| {
            format!("snapshot for L1 batch #{l1_batch_number} is not present on main node")
        })?;
        let l2_block_number = snapshot.l2_block_number;
        tracing::info!(
            "Found snapshot with data up to L1 batch #{l1_batch_number}, L2 block #{l2_block_number}, \
            version {version}, storage logs are divided into {chunk_count} chunk(s)",
            version = snapshot.version,
            chunk_count = snapshot.storage_logs_chunks.len()
        );
        let snapshot_version = Self::check_snapshot_version(snapshot.version)?;

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

        let status = SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_timestamp: l1_batch.base.timestamp,
            l1_batch_root_hash,
            l2_block_number: snapshot.l2_block_number,
            l2_block_timestamp: l2_block.base.timestamp,
            l2_block_hash,
            protocol_version,
            storage_logs_chunks_processed: vec![false; snapshot.storage_logs_chunks.len()],
        };
        Ok((status, snapshot_version))
    }

    fn check_snapshot_version(raw_version: u16) -> anyhow::Result<SnapshotVersion> {
        let version = SnapshotVersion::try_from(raw_version).with_context(|| {
            format!(
                "Unrecognized snapshot version: {raw_version}; make sure you're running the latest version of the node"
            )
        })?;
        anyhow::ensure!(
            matches!(version, SnapshotVersion::Version0 | SnapshotVersion::Version1),
            "Cannot recover from a snapshot with version {version:?}; the only supported versions are {:?}",
            [SnapshotVersion::Version0, SnapshotVersion::Version1]
        );
        Ok(version)
    }
}

/// Versioned storage logs chunk.
#[derive(Debug)]
enum StorageLogs {
    V0(Vec<SnapshotStorageLog<StorageKey>>),
    V1(Vec<SnapshotStorageLog>),
}

impl StorageLogs {
    async fn load(
        blob_store: &dyn ObjectStore,
        key: SnapshotStorageLogsStorageKey,
        version: SnapshotVersion,
    ) -> Result<Self, ObjectStoreError> {
        match version {
            SnapshotVersion::Version0 => {
                let logs: SnapshotStorageLogsChunk<StorageKey> = blob_store.get(key).await?;
                Ok(Self::V0(logs.storage_logs))
            }
            SnapshotVersion::Version1 => {
                let logs: SnapshotStorageLogsChunk = blob_store.get(key).await?;
                Ok(Self::V1(logs.storage_logs))
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::V0(logs) => logs.len(),
            Self::V1(logs) => logs.len(),
        }
    }

    /// Performs basic sanity check for a storage logs chunk.
    fn validate(&self, snapshot_status: &SnapshotRecoveryStatus) -> anyhow::Result<()> {
        match self {
            Self::V0(logs) => Self::validate_inner(logs, snapshot_status),
            Self::V1(logs) => Self::validate_inner(logs, snapshot_status),
        }
    }

    fn validate_inner<K: fmt::Debug>(
        storage_logs: &[SnapshotStorageLog<K>],
        snapshot_status: &SnapshotRecoveryStatus,
    ) -> anyhow::Result<()> {
        for log in storage_logs {
            anyhow::ensure!(
                log.enumeration_index > 0,
                "invalid storage log with zero enumeration_index: {log:?}"
            );
            anyhow::ensure!(
                log.l1_batch_number_of_initial_write <= snapshot_status.l1_batch_number,
                "invalid storage log with `l1_batch_number_of_initial_write` from the future: {log:?}"
            );
        }
        Ok(())
    }

    fn drop_key_preimages(&mut self) {
        match self {
            Self::V0(logs) => {
                *self = Self::V1(
                    mem::take(logs)
                        .into_iter()
                        .map(SnapshotStorageLog::drop_key_preimage)
                        .collect(),
                );
            }
            Self::V1(_) => { /* do nothing */ }
        }
    }

    fn without_preimages(self) -> Vec<SnapshotStorageLog> {
        match self {
            Self::V0(logs) => logs
                .into_iter()
                .map(SnapshotStorageLog::drop_key_preimage)
                .collect(),
            Self::V1(logs) => logs,
        }
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
    snapshot_version: SnapshotVersion,
    max_concurrency: usize,
    drop_storage_key_preimages: bool,
    factory_deps_recovered: bool,
    tokens_recovered: bool,
}

#[derive(Debug)]
enum SnapshotsApplierOrStatus<'a> {
    Applier(SnapshotsApplier<'a>, SnapshotRecoveryStrategy),
    CompletedStatus(SnapshotRecoveryStatus),
}

impl<'a> SnapshotsApplierOrStatus<'a> {
    /// Returns final snapshot recovery status.
    async fn load_snapshot(
        task: &'a SnapshotsApplierTask,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> StopResult<(SnapshotRecoveryStrategy, SnapshotRecoveryStatus)> {
        let (mut applier, strategy) = match Self::new(task).await? {
            Self::Applier(applier, strategy) => (applier, strategy),
            Self::CompletedStatus(status) => {
                return Ok((SnapshotRecoveryStrategy::Completed, status))
            }
        };
        applier.recover_storage_logs(stop_receiver).await?;
        for is_chunk_processed in &mut applier
            .applied_snapshot_status
            .storage_logs_chunks_processed
        {
            *is_chunk_processed = true;
        }

        applier.recover_tokens().await?;
        applier.tokens_recovered = true;
        applier.update_health();
        Ok((strategy, applier.applied_snapshot_status))
    }

    async fn new(task: &'a SnapshotsApplierTask) -> Result<Self, SnapshotsApplierError> {
        let health_updater = &task.health_updater;
        let connection_pool = &task.connection_pool;
        let main_node_client = task.main_node_client.as_ref();

        // While the recovery is in progress, the node is healthy (no error has occurred),
        // but is affected (its usual APIs don't work).
        health_updater.update(HealthStatus::Affected.into());

        let mut storage = connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await?;

        let (strategy, applied_snapshot_status) = SnapshotRecoveryStrategy::new(
            &mut storage_transaction,
            main_node_client,
            task.snapshot_l1_batch,
        )
        .await?;
        tracing::info!("Chosen snapshot recovery strategy: {strategy:?} with status: {applied_snapshot_status:?}");
        let (created_from_scratch, snapshot_version) = match strategy {
            SnapshotRecoveryStrategy::New(version) => (true, version),
            SnapshotRecoveryStrategy::Resumed(version) => (false, version),
            SnapshotRecoveryStrategy::Completed => {
                return Ok(Self::CompletedStatus(applied_snapshot_status))
            }
        };

        let mut applier = SnapshotsApplier {
            connection_pool,
            main_node_client,
            blob_store: task.blob_store.as_ref(),
            applied_snapshot_status,
            health_updater,
            snapshot_version,
            max_concurrency: task.config.max_concurrency.get(),
            drop_storage_key_preimages: task.drop_storage_key_preimages,
            factory_deps_recovered: !created_from_scratch,
            tokens_recovered: false,
        };

        METRICS.storage_logs_chunks_count.set(
            applier
                .applied_snapshot_status
                .storage_logs_chunks_processed
                .len(),
        );
        METRICS.storage_logs_chunks_left_to_process.set(
            applier
                .applied_snapshot_status
                .storage_logs_chunks_left_to_process(),
        );
        applier.update_health();

        if created_from_scratch {
            applier
                .recover_factory_deps(&mut storage_transaction)
                .await?;
            storage_transaction
                .snapshot_recovery_dal()
                .insert_initial_recovery_status(&applier.applied_snapshot_status)
                .await?;

            // Insert artificial entries into the pruning log so that it's guaranteed to match the snapshot recovery metadata.
            // This allows to not deal with the corner cases when a node was recovered from a snapshot, but its pruning log is empty.
            storage_transaction
                .pruning_dal()
                .insert_soft_pruning_log(
                    applier.applied_snapshot_status.l1_batch_number,
                    applier.applied_snapshot_status.l2_block_number,
                )
                .await?;
            storage_transaction
                .pruning_dal()
                .insert_hard_pruning_log(
                    applier.applied_snapshot_status.l1_batch_number,
                    applier.applied_snapshot_status.l2_block_number,
                    applier.applied_snapshot_status.l1_batch_root_hash,
                )
                .await?;
        }
        storage_transaction.commit().await?;
        drop(storage);
        applier.factory_deps_recovered = true;
        applier.update_health();
        Ok(Self::Applier(applier, strategy))
    }
}

impl<'a> SnapshotsApplier<'a> {
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

        // we cannot insert all factory deps because of field size limit triggered by UNNEST
        // in underlying query, see `https://www.postgresql.org/docs/current/limits.html`
        // there were around 100 thousand contracts on mainnet, where this issue first manifested
        for chunk in factory_deps.factory_deps.chunks(1000) {
            let chunk_deps_hashmap: HashMap<H256, Vec<u8>> = chunk
                .iter()
                .map(|dep| {
                    let bytecode_hash = if let Some(hash) = dep.hash {
                        // Sanity-check the bytecode hash.
                        let parsed_hash = BytecodeHash::try_from(hash)
                            .with_context(|| format!("for bytecode hash {hash:?}"))?;
                        let restored_hash = match parsed_hash.marker() {
                            BytecodeMarker::EraVm => BytecodeHash::for_bytecode(&dep.bytecode.0),
                            BytecodeMarker::Evm => BytecodeHash::for_evm_bytecode(
                                parsed_hash.len_in_bytes(),
                                &dep.bytecode.0,
                            ),
                        };
                        anyhow::ensure!(
                            parsed_hash == restored_hash,
                            "restored bytecode hash {restored_hash:?} doesn't match hash from the snapshot {parsed_hash:?}"
                        );

                        hash
                    } else {
                        // Assume that this is an EraVM bytecode from an old snapshot. Note that EVM bytecodes in Postgres and snapshots are padded,
                        // so we wouldn't be able to conclusively get the bytecode length, even if we can more or less conclusively tell that it is
                        // an EVM bytecode (EraVM bytecodes usually start with a 0 byte, EVM bytecodes almost never do).
                        if dep.bytecode.0[0] != 0 {
                            tracing::warn!(
                                "Potential EVM bytecode included into an old snapshot: {:?}. If snapshot recovery fails, please use a newer snapshot",
                                dep.bytecode
                            );
                        }
                        BytecodeHash::for_bytecode(&dep.bytecode.0).value()
                    };

                    Ok((bytecode_hash, dep.bytecode.0.clone()))
                })
                .collect::<anyhow::Result<_>>()?;
            storage
                .factory_deps_dal()
                .insert_factory_deps(
                    self.applied_snapshot_status.l2_block_number,
                    &chunk_deps_hashmap,
                )
                .await?;
        }

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
        storage_logs: &StorageLogs,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), SnapshotsApplierError> {
        match storage_logs {
            StorageLogs::V0(logs) => {
                #[allow(deprecated)]
                storage
                    .storage_logs_dal()
                    .insert_storage_logs_with_preimages_from_snapshot(
                        self.applied_snapshot_status.l2_block_number,
                        logs,
                    )
                    .await?;
            }
            StorageLogs::V1(logs) => {
                storage
                    .storage_logs_dal()
                    .insert_storage_logs_from_snapshot(
                        self.applied_snapshot_status.l2_block_number,
                        logs,
                    )
                    .await?;
            }
        }
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
        let mut storage_logs =
            StorageLogs::load(self.blob_store, storage_key, self.snapshot_version)
                .await
                .map_err(|err| {
                    let context =
                        format!("cannot fetch storage logs {storage_key:?} from object store");
                    SnapshotsApplierError::object_store(err, context)
                })?;

        storage_logs.validate(&self.applied_snapshot_status)?;
        if self.drop_storage_key_preimages {
            storage_logs.drop_key_preimages();
        }
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

        self.insert_storage_logs_chunk(&storage_logs, &mut storage_transaction)
            .await?;
        let storage_logs = storage_logs.without_preimages();
        self.insert_initial_writes_chunk(&storage_logs, &mut storage_transaction)
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

    async fn recover_storage_logs(
        &self,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> StopResult<()> {
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
        let job_completion = futures::future::try_join_all(tasks);

        tokio::select! {
            res = job_completion => {
                res?;
            },
            _ = stop_receiver.changed() => {
                return Err(OrStopped::Stopped);
            }
        }

        self.finalize_processing_storage_logs().await?;
        Ok(())
    }

    async fn finalize_processing_storage_logs(&self) -> Result<(), SnapshotsApplierError> {
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
            return Err(err.into());
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

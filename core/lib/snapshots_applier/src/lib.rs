//! Logic for applying application-level snapshots to Postgres storage.

use std::{collections::HashMap, fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Semaphore;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, SqlxError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::{
    api::en::SyncBlock,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    tokens::TokenInfo,
    web3::futures,
    L1BatchNumber, MiniblockNumber, H256,
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

#[derive(Debug, Serialize)]
struct SnapshotsApplierHealthDetails {
    snapshot_miniblock: MiniblockNumber,
    snapshot_l1_batch: L1BatchNumber,
    factory_deps_recovered: bool,
    storage_logs_chunk_count: usize,
    storage_logs_chunks_left_to_process: usize,
    tokens_recovered: bool,
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
    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
    ) -> EnrichedClientResult<Option<SyncBlock>>;

    async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>>;

    async fn fetch_tokens(
        &self,
        at_miniblock: MiniblockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>>;
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

    async fn fetch_tokens(
        &self,
        at_miniblock: MiniblockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        self.sync_tokens(Some(at_miniblock))
            .rpc_context("sync_tokens")
            .with_arg("miniblock_number", &at_miniblock)
            .await
    }
}

/// Snapshot applier configuration options.
#[derive(Debug)]
pub struct SnapshotsApplierConfig {
    pub retry_count: usize,
    pub initial_retry_backoff: Duration,
    pub retry_backoff_multiplier: f32,
    health_updater: HealthUpdater,
}

impl Default for SnapshotsApplierConfig {
    fn default() -> Self {
        Self {
            retry_count: 5,
            initial_retry_backoff: Duration::from_secs(2),
            retry_backoff_multiplier: 2.0,
            health_updater: ReactiveHealthCheck::new("snapshot_recovery").1,
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
    /// - Storage contains at least one L1 batch
    pub async fn run(
        self,
        connection_pool: &ConnectionPool<Core>,
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
        blob_store: &dyn ObjectStore,
    ) -> anyhow::Result<()> {
        let mut backoff = self.initial_retry_backoff;
        let mut last_error = None;
        for retry_id in 0..self.retry_count {
            let result = SnapshotsApplier::load_snapshot(
                connection_pool,
                main_node_client,
                blob_store,
                &self.health_updater,
            )
            .await;

            match result {
                Ok(()) => {
                    // Freeze the health check in the "ready" status, so that the snapshot recovery isn't marked
                    // as "shut down", which would lead to the app considered unhealthy.
                    self.health_updater.freeze();
                    return Ok(());
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
    connection_pool: &'a ConnectionPool<Core>,
    main_node_client: &'a dyn SnapshotsApplierMainNodeClient,
    blob_store: &'a dyn ObjectStore,
    applied_snapshot_status: SnapshotRecoveryStatus,
    health_updater: &'a HealthUpdater,
    factory_deps_recovered: bool,
    tokens_recovered: bool,
}

impl<'a> SnapshotsApplier<'a> {
    /// Recovers [`SnapshotRecoveryStatus`] from the storage and the main node.
    async fn prepare_applied_snapshot_status(
        storage: &mut Connection<'_, Core>,
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
                let err = anyhow::anyhow!(
                    "node contains a non-genesis L1 batch; snapshot recovery is unsafe"
                );
                return Err(SnapshotsApplierError::Fatal(err));
            }

            let recovery_status =
                SnapshotsApplier::create_fresh_recovery_status(main_node_client).await?;

            let storage_logs_count = storage
                .storage_logs_dal()
                .get_storage_logs_row_count(recovery_status.miniblock_number)
                .await
                .map_err(|err| {
                    SnapshotsApplierError::db(err, "cannot get storage_logs row count")
                })?;
            if storage_logs_count > 0 {
                let err = anyhow::anyhow!(
                    "storage_logs table has {storage_logs_count} rows at or before the snapshot miniblock #{}; \
                     snapshot recovery is unsafe",
                    recovery_status.miniblock_number
                );
                return Err(SnapshotsApplierError::Fatal(err));
            }

            let latency = latency.observe();
            tracing::info!("Initialized fresh snapshots applier in {latency:?}");
            Ok((recovery_status, true))
        }
    }

    async fn load_snapshot(
        connection_pool: &'a ConnectionPool<Core>,
        main_node_client: &'a dyn SnapshotsApplierMainNodeClient,
        blob_store: &'a dyn ObjectStore,
        health_updater: &'a HealthUpdater,
    ) -> Result<(), SnapshotsApplierError> {
        health_updater.update(HealthStatus::Ready.into());

        let mut storage = connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        let mut storage_transaction = storage.start_transaction().await.map_err(|err| {
            SnapshotsApplierError::db(err, "failed starting initial DB transaction")
        })?;

        if storage_transaction
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .map_err(|err| SnapshotsApplierError::db(err, "failed to check need of recovery"))?
            .is_some()
            && storage_transaction
                .blocks_dal()
                .get_sealed_miniblock_number()
                .await
                .map_err(|err| SnapshotsApplierError::db(err, "failed to check need of recovery"))?
                .is_some()
        {
            return Ok(());
        }

        let (applied_snapshot_status, created_from_scratch) =
            Self::prepare_applied_snapshot_status(&mut storage_transaction, main_node_client)
                .await?;

        let mut this = Self {
            connection_pool,
            main_node_client,
            blob_store,
            applied_snapshot_status,
            health_updater,
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
                .await
                .map_err(|err| {
                    SnapshotsApplierError::db(err, "failed persisting initial recovery status")
                })?;
            storage_transaction
                .pruning_dal()
                .soft_prune_batches_range(
                    this.applied_snapshot_status.l1_batch_number,
                    this.applied_snapshot_status.miniblock_number,
                )
                .await
                .map_err(|err| SnapshotsApplierError::db(err, "failed inserting pruning info"))?;

            storage_transaction
                .pruning_dal()
                .hard_prune_batches_range(
                    this.applied_snapshot_status.l1_batch_number,
                    this.applied_snapshot_status.miniblock_number,
                )
                .await
                .map_err(|err| SnapshotsApplierError::db(err, "failed inserting pruning info"))?;
        }
        storage_transaction.commit().await.map_err(|err| {
            SnapshotsApplierError::db(err, "failed committing initial DB transaction")
        })?;
        drop(storage);
        this.factory_deps_recovered = true;
        this.update_health();

        this.recover_storage_logs().await?;
        this.recover_tokens().await?;
        this.tokens_recovered = true;
        this.update_health();

        Ok(())
    }

    async fn create_fresh_recovery_status(
        main_node_client: &dyn SnapshotsApplierMainNodeClient,
    ) -> Result<SnapshotRecoveryStatus, SnapshotsApplierError> {
        let snapshot_response = main_node_client.fetch_newest_snapshot().await?;

        let snapshot = snapshot_response
            .context("no snapshots on main node; snapshot recovery is impossible")?;
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

    fn update_health(&self) {
        let details = SnapshotsApplierHealthDetails {
            snapshot_miniblock: self.applied_snapshot_status.miniblock_number,
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
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(details));
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
        storage: &mut Connection<'_, Core>,
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
        storage: &mut Connection<'_, Core>,
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

        let mut storage = self
            .connection_pool
            .connection_tagged("snapshots_applier")
            .await?;
        // This DB query is slow, but this is fine for verification purposes.
        let total_log_count = storage
            .storage_logs_dal()
            .get_storage_logs_row_count(self.applied_snapshot_status.miniblock_number)
            .await
            .map_err(|err| SnapshotsApplierError::db(err, "cannot get storage_logs row count"))?;
        tracing::info!(
            "Recovered {total_log_count} storage logs in total; checking overall consistency..."
        );

        let number_of_logs_by_enum_indices = storage
            .snapshots_creator_dal()
            .get_distinct_storage_logs_keys_count(self.applied_snapshot_status.l1_batch_number)
            .await
            .map_err(|err| {
                SnapshotsApplierError::db(err, "cannot get storage log count by initial writes")
            })?;
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
        let all_token_addresses = storage
            .tokens_dal()
            .get_all_l2_token_addresses()
            .await
            .map_err(|err| SnapshotsApplierError::db(err, "failed fetching L2 token addresses"))?;
        if !all_token_addresses.is_empty() {
            tracing::info!(
                "{} tokens are already present in DB; skipping token recovery",
                all_token_addresses.len()
            );
            return Ok(());
        }
        drop(storage);

        let snapshot_miniblock_number = self.applied_snapshot_status.miniblock_number;
        let tokens = self
            .main_node_client
            .fetch_tokens(snapshot_miniblock_number)
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
            .filter_deployed_contracts(l2_addresses, Some(snapshot_miniblock_number))
            .await
            .map_err(|err| {
                SnapshotsApplierError::db(err, "failed querying L2 contracts for tokens")
            })?;

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
        storage
            .tokens_dal()
            .add_tokens(&tokens)
            .await
            .map_err(|err| SnapshotsApplierError::db(err, "failed persisting tokens"))?;
        Ok(())
    }
}

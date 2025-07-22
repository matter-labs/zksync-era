//! [`SnapshotCreator`] and tightly related types.

use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::sync::Semaphore;
use zksync_config::SnapshotsCreatorConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalResult};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_types::{
    snapshots::{
        uniform_hashed_keys_chunk, SnapshotFactoryDependencies, SnapshotFactoryDependency,
        SnapshotMetadata, SnapshotStorageLog, SnapshotStorageLogsChunk,
        SnapshotStorageLogsStorageKey, SnapshotVersion,
    },
    L1BatchNumber, L2BlockNumber,
};

use crate::metrics::{FactoryDepsStage, StorageChunkStage, METRICS};
#[cfg(test)]
use crate::tests::HandleEvent;

/// Encapsulates progress of creating a particular storage snapshot.
#[derive(Debug)]
struct SnapshotProgress {
    version: SnapshotVersion,
    l1_batch_number: L1BatchNumber,
    /// `true` if the snapshot is new (i.e., its progress is not recovered from Postgres).
    is_new_snapshot: bool,
    chunk_count: u64,
    remaining_chunk_ids: Vec<u64>,
}

impl SnapshotProgress {
    fn new(version: SnapshotVersion, l1_batch_number: L1BatchNumber, chunk_count: u64) -> Self {
        Self {
            version,
            l1_batch_number,
            is_new_snapshot: true,
            chunk_count,
            remaining_chunk_ids: (0..chunk_count).collect(),
        }
    }

    fn from_existing_snapshot(snapshot: &SnapshotMetadata) -> Self {
        let remaining_chunk_ids = snapshot
            .storage_logs_filepaths
            .iter()
            .enumerate()
            .filter_map(|(chunk_id, path)| path.is_none().then_some(chunk_id as u64))
            .collect();

        Self {
            version: snapshot.version,
            l1_batch_number: snapshot.l1_batch_number,
            is_new_snapshot: false,
            chunk_count: snapshot.storage_logs_filepaths.len() as u64,
            remaining_chunk_ids,
        }
    }
}

/// Creator of a single storage snapshot.
#[derive(Debug)]
pub(crate) struct SnapshotCreator {
    pub blob_store: Arc<dyn ObjectStore>,
    pub master_pool: ConnectionPool<Core>,
    pub replica_pool: ConnectionPool<Core>,
    #[cfg(test)]
    pub event_listener: Box<dyn HandleEvent>,
}

impl SnapshotCreator {
    async fn connect_to_replica(&self) -> DalResult<Connection<'_, Core>> {
        self.replica_pool
            .connection_tagged("snapshots_creator")
            .await
    }

    async fn process_storage_logs_single_chunk(
        &self,
        semaphore: &Semaphore,
        progress: &SnapshotProgress,
        l2_block_number: L2BlockNumber,
        chunk_id: u64,
    ) -> anyhow::Result<()> {
        let chunk_count = progress.chunk_count;
        let l1_batch_number = progress.l1_batch_number;

        let _permit = semaphore.acquire().await?;
        #[cfg(test)]
        if self.event_listener.on_chunk_started().should_exit() {
            return Ok(());
        }

        let hashed_keys_range = uniform_hashed_keys_chunk(chunk_id, chunk_count);
        let mut conn = self.connect_to_replica().await?;

        let latency =
            METRICS.storage_logs_processing_duration[&StorageChunkStage::LoadFromPostgres].start();
        let (output_filepath, latency) = match progress.version {
            SnapshotVersion::Version0 => {
                #[allow(deprecated)] // support of version 0 snapshots will be removed eventually
                let logs = conn
                    .snapshots_creator_dal()
                    .get_storage_logs_chunk_with_key_preimages(
                        l2_block_number,
                        l1_batch_number,
                        hashed_keys_range,
                    )
                    .await
                    .context("error fetching storage logs")?;
                drop(conn);

                let latency = latency.observe();
                tracing::info!(
                    "Loaded chunk {chunk_id} ({} logs) from Postgres in {latency:?}",
                    logs.len()
                );
                self.store_storage_logs_chunk(l1_batch_number, chunk_id, logs)
                    .await?
            }
            SnapshotVersion::Version1 => {
                let logs = conn
                    .snapshots_creator_dal()
                    .get_storage_logs_chunk(l2_block_number, l1_batch_number, hashed_keys_range)
                    .await
                    .context("error fetching storage logs")?;
                drop(conn);

                let latency = latency.observe();
                tracing::info!(
                    "Loaded chunk {chunk_id} ({} logs) from Postgres in {latency:?}",
                    logs.len()
                );
                self.store_storage_logs_chunk(l1_batch_number, chunk_id, logs)
                    .await?
            }
        };

        let mut master_conn = self
            .master_pool
            .connection_tagged("snapshots_creator")
            .await?;
        master_conn
            .snapshots_dal()
            .add_storage_logs_filepath_for_snapshot(l1_batch_number, chunk_id, &output_filepath)
            .await?;
        #[cfg(test)]
        self.event_listener.on_chunk_saved();

        let tasks_left = METRICS.storage_logs_chunks_left_to_process.dec_by(1) - 1;
        tracing::info!(
            "Saved chunk {chunk_id} (overall progress {}/{chunk_count}) in {latency:?} to location: {output_filepath}",
            chunk_count - tasks_left as u64
        );
        Ok(())
    }

    async fn store_storage_logs_chunk<K>(
        &self,
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        logs: Vec<SnapshotStorageLog<K>>,
    ) -> anyhow::Result<(String, Duration)>
    where
        for<'a> SnapshotStorageLogsChunk<K>: StoredObject<Key<'a> = SnapshotStorageLogsStorageKey>,
    {
        let latency =
            METRICS.storage_logs_processing_duration[&StorageChunkStage::SaveToGcs].start();
        let storage_logs_chunk = SnapshotStorageLogsChunk { storage_logs: logs };
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number,
            chunk_id,
        };
        let filename = self
            .blob_store
            .put(key, &storage_logs_chunk)
            .await
            .context("Error storing storage logs chunk in blob store")?;
        let output_filepath_prefix = self
            .blob_store
            .get_storage_prefix::<SnapshotStorageLogsChunk<K>>();
        let output_filepath = format!("{output_filepath_prefix}/{filename}");
        let latency = latency.observe();
        Ok((output_filepath, latency))
    }

    async fn process_factory_deps(
        &self,
        l2_block_number: L2BlockNumber,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<String> {
        let mut conn = self.connect_to_replica().await?;

        tracing::info!("Loading factory deps from Postgres...");
        let latency =
            METRICS.factory_deps_processing_duration[&FactoryDepsStage::LoadFromPostgres].start();
        let factory_deps = conn
            .snapshots_creator_dal()
            .get_all_factory_deps(l2_block_number)
            .await?;
        drop(conn);
        let latency = latency.observe();
        tracing::info!("Loaded {} factory deps in {latency:?}", factory_deps.len());

        tracing::info!("Saving factory deps to GCS...");
        let latency =
            METRICS.factory_deps_processing_duration[&FactoryDepsStage::SaveToGcs].start();
        let factory_deps = factory_deps
            .into_iter()
            .map(|(hash, bytecode)| SnapshotFactoryDependency {
                bytecode: bytecode.into(),
                hash: Some(hash),
            })
            .collect();
        let factory_deps = SnapshotFactoryDependencies { factory_deps };
        let filename = self
            .blob_store
            .put(l1_batch_number, &factory_deps)
            .await
            .context("Error storing factory deps in blob store")?;
        let output_filepath_prefix = self
            .blob_store
            .get_storage_prefix::<SnapshotFactoryDependencies>();
        let output_filepath = format!("{output_filepath_prefix}/{filename}");
        let latency = latency.observe();
        tracing::info!(
            "Saved {} factory deps in {latency:?} to location: {output_filepath}",
            factory_deps.factory_deps.len()
        );

        Ok(output_filepath)
    }

    /// Returns `Ok(None)` if the created snapshot would coincide with `latest_snapshot`.
    async fn initialize_snapshot_progress(
        config: &SnapshotsCreatorConfig,
        l1_batch_number: L1BatchNumber,
        min_chunk_count: u64,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<Option<SnapshotProgress>> {
        let snapshot_version = SnapshotVersion::try_from(config.version)
            .context("invalid snapshot version specified in config")?;

        // Sanity check: the selected L1 batch should have Merkle tree data; otherwise, it could be impossible
        // to recover from the generated snapshot.
        conn.blocks_dal()
            .get_l1_batch_tree_data(l1_batch_number)
            .await?
            .with_context(|| {
                format!(
                    "Snapshot L1 batch #{l1_batch_number} doesn't have tree data, meaning recovery from the snapshot \
                     could be impossible. This should never happen"
                )
            })?;

        let distinct_storage_logs_keys_count = conn
            .snapshots_creator_dal()
            .get_distinct_storage_logs_keys_count(l1_batch_number)
            .await?;
        let chunk_size = config.storage_logs_chunk_size;
        // We force the minimum number of chunks to avoid situations where only one chunk is created in tests.
        let chunk_count = distinct_storage_logs_keys_count
            .div_ceil(chunk_size)
            .max(min_chunk_count);

        tracing::info!(
            "Selected storage logs chunking for L1 batch {l1_batch_number}: \
            {chunk_count} chunks of expected size {chunk_size}"
        );
        Ok(Some(SnapshotProgress::new(
            snapshot_version,
            l1_batch_number,
            chunk_count,
        )))
    }

    /// Returns `Ok(None)` if a snapshot should not be created / resumed.
    async fn load_or_initialize_snapshot_progress(
        &self,
        config: &SnapshotsCreatorConfig,
        min_chunk_count: u64,
    ) -> anyhow::Result<Option<SnapshotProgress>> {
        let mut master_conn = self
            .master_pool
            .connection_tagged("snapshots_creator")
            .await?;

        let sealed_l1_batch_number = master_conn
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?;
        let sealed_l1_batch_number = sealed_l1_batch_number.context("No L1 batches in Postgres")?;
        let (requested_l1_batch_number, existing_snapshot) = if let Some(l1_batch_number) =
            config.l1_batch_number
        {
            anyhow::ensure!(
                l1_batch_number <= sealed_l1_batch_number,
                "Requested a snapshot for L1 batch #{l1_batch_number} that doesn't exist in Postgres (latest L1 batch: {sealed_l1_batch_number})"
            );

            let existing_snapshot = master_conn
                .snapshots_dal()
                .get_snapshot_metadata(l1_batch_number)
                .await?;
            (l1_batch_number, existing_snapshot)
        } else {
            // We subtract 1 so that after restore, EN node has at least one L1 batch to fetch.
            anyhow::ensure!(
                sealed_l1_batch_number != L1BatchNumber(0),
                "Cannot create snapshot when only the genesis L1 batch is present in Postgres"
            );
            let requested_l1_batch_number = sealed_l1_batch_number - 1;

            // Continue creating a pending snapshot if it exists, even if it doesn't correspond to the latest L1 batch.
            // OTOH, a completed snapshot does not matter, unless it corresponds to `requested_l1_batch_number` (in which case it doesn't need to be created again).
            let existing_snapshot = master_conn
                .snapshots_dal()
                .get_newest_snapshot_metadata()
                .await?
                .filter(|snapshot| {
                    !snapshot.is_complete() || snapshot.l1_batch_number == requested_l1_batch_number
                });
            (requested_l1_batch_number, existing_snapshot)
        };
        drop(master_conn);

        match existing_snapshot {
            Some(snapshot) if snapshot.is_complete() => {
                tracing::info!("Snapshot for the requested L1 batch is complete: {snapshot:?}");
                Ok(None)
            }
            Some(snapshot) => Ok(Some(SnapshotProgress::from_existing_snapshot(&snapshot))),
            None => {
                Self::initialize_snapshot_progress(
                    config,
                    requested_l1_batch_number,
                    min_chunk_count,
                    &mut self.connect_to_replica().await?,
                )
                .await
            }
        }
    }

    pub async fn run(
        self,
        config: SnapshotsCreatorConfig,
        min_chunk_count: u64,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Starting snapshot creator with object store {:?} and config {config:?}",
            self.blob_store
        );
        let latency = METRICS.snapshot_generation_duration.start();

        let Some(progress) = self
            .load_or_initialize_snapshot_progress(&config, min_chunk_count)
            .await?
        else {
            // No snapshot creation is necessary; a snapshot for the current L1 batch is already created
            return Ok(());
        };

        let mut conn = self.connect_to_replica().await?;
        let (_, last_l2_block_number_in_batch) = conn
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(progress.l1_batch_number)
            .await?
            .context("No L2 blocks for L1 batch")?;
        drop(conn);

        METRICS.storage_logs_chunks_count.set(progress.chunk_count);
        tracing::info!(
            "Creating snapshot for storage logs up to L2 block {last_l2_block_number_in_batch}, \
            L1 batch {}",
            progress.l1_batch_number
        );

        if progress.is_new_snapshot {
            let factory_deps_output_file = self
                .process_factory_deps(last_l2_block_number_in_batch, progress.l1_batch_number)
                .await?;

            let mut master_conn = self
                .master_pool
                .connection_tagged("snapshots_creator")
                .await?;
            master_conn
                .snapshots_dal()
                .add_snapshot(
                    progress.version,
                    progress.l1_batch_number,
                    progress.chunk_count,
                    &factory_deps_output_file,
                )
                .await?;
        }

        METRICS
            .storage_logs_chunks_left_to_process
            .set(progress.remaining_chunk_ids.len());
        let semaphore = Semaphore::new(config.concurrent_queries_count as usize);
        let tasks = progress
            .remaining_chunk_ids
            .iter()
            .copied()
            .map(|chunk_id| {
                self.process_storage_logs_single_chunk(
                    &semaphore,
                    &progress,
                    last_l2_block_number_in_batch,
                    chunk_id,
                )
            });
        futures::future::try_join_all(tasks).await?;

        METRICS
            .snapshot_l1_batch
            .set(progress.l1_batch_number.0.into());

        let elapsed = latency.observe();
        tracing::info!("snapshot_generation_duration: {elapsed:?}");
        tracing::info!("snapshot_l1_batch: {}", METRICS.snapshot_l1_batch.get());
        tracing::info!(
            "storage_logs_chunks_count: {}",
            METRICS.storage_logs_chunks_count.get()
        );
        Ok(())
    }
}

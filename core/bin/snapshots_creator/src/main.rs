//! Snapshot creator utility. Intended to run on a schedule, with each run creating a new snapshot.

use std::collections::HashSet;

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::{watch, Semaphore};
use zksync_config::{configs::PrometheusConfig, PostgresConfig, SnapshotsCreatorConfig};
use zksync_dal::ConnectionPool;
use zksync_env_config::{object_store::SnapshotsObjectStoreConfig, FromEnv};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotMetadata, SnapshotStorageLogsChunk,
        SnapshotStorageLogsStorageKey,
    },
    L1BatchNumber, MiniblockNumber,
};
use zksync_utils::ceil_div;

#[cfg(test)]
use crate::tests::HandleEvent;
use crate::{
    chunking::get_chunk_hashed_keys_range,
    metrics::{FactoryDepsStage, StorageChunkStage, METRICS},
};

mod chunking;
mod metrics;
#[cfg(test)]
mod tests;

async fn maybe_enable_prometheus_metrics(
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let prometheus_config = PrometheusConfig::from_env().ok();
    if let Some(prometheus_config) = prometheus_config {
        let exporter_config = PrometheusExporterConfig::push(
            prometheus_config.gateway_endpoint(),
            prometheus_config.push_interval(),
        );

        tracing::info!("Starting prometheus exporter with config {prometheus_config:?}");
        tokio::spawn(exporter_config.run(stop_receiver));
    } else {
        tracing::info!("Starting without prometheus exporter");
    }
    Ok(())
}

/// Encapsulates progress of creating a particular storage snapshot.
#[derive(Debug)]
struct SnapshotProgress {
    l1_batch_number: L1BatchNumber,
    needs_persisting_factory_deps: bool,
    chunk_count: u64,
    remaining_chunk_ids: Vec<u64>,
}

impl SnapshotProgress {
    fn new(l1_batch_number: L1BatchNumber, chunk_count: u64) -> Self {
        Self {
            l1_batch_number,
            needs_persisting_factory_deps: true,
            chunk_count,
            remaining_chunk_ids: (0..chunk_count).collect(),
        }
    }

    fn from_existing_snapshot(
        snapshot: &SnapshotMetadata,
        blob_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self> {
        let output_filepath_prefix = blob_store.get_storage_prefix::<SnapshotStorageLogsChunk>();
        let existing_chunk_ids = snapshot.storage_logs_filepaths.iter().map(|path| {
            // FIXME: this parsing looks unnecessarily fragile
            let object_key = path
                .strip_prefix(&output_filepath_prefix)
                .with_context(|| format!("Path `{path}` has unexpected prefix"))?;
            let object_key = object_key
                .strip_prefix('/')
                .with_context(|| format!("Path `{path}` has unexpected prefix"))?;
            let object_key = object_key
                .parse::<SnapshotStorageLogsStorageKey>()
                .with_context(|| format!("Object key `{object_key}` cannot be parsed"))?;
            anyhow::ensure!(
                object_key.l1_batch_number == snapshot.l1_batch_number,
                "Mismatch"
            );
            anyhow::Ok(object_key.chunk_id)
        });
        let existing_chunk_ids: anyhow::Result<HashSet<_>> = existing_chunk_ids.collect();
        let existing_chunk_ids = existing_chunk_ids?;

        let all_chunk_ids = (0..snapshot.storage_logs_chunk_count).collect::<HashSet<_>>();
        let remaining_chunk_ids = all_chunk_ids
            .difference(&existing_chunk_ids)
            .copied()
            .collect();

        Ok(Self {
            l1_batch_number: snapshot.l1_batch_number,
            needs_persisting_factory_deps: false,
            chunk_count: snapshot.storage_logs_chunk_count,
            remaining_chunk_ids,
        })
    }
}

/// Creator of a single storage snapshot.
#[derive(Debug)]
struct SnapshotCreator {
    blob_store: Box<dyn ObjectStore>,
    master_pool: ConnectionPool,
    replica_pool: ConnectionPool,
    #[cfg(test)]
    event_listener: Box<dyn HandleEvent>,
}

impl SnapshotCreator {
    async fn process_storage_logs_single_chunk(
        &self,
        semaphore: &Semaphore,
        miniblock_number: MiniblockNumber,
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        chunk_count: u64,
    ) -> anyhow::Result<()> {
        let _permit = semaphore.acquire().await?;
        #[cfg(test)]
        if self.event_listener.on_chunk_started().should_exit() {
            return Ok(());
        }

        let hashed_keys_range = get_chunk_hashed_keys_range(chunk_id, chunk_count);
        let mut conn = self
            .replica_pool
            .access_storage_tagged("snapshots_creator")
            .await?;

        let latency =
            METRICS.storage_logs_processing_duration[&StorageChunkStage::LoadFromPostgres].start();
        let logs = conn
            .snapshots_creator_dal()
            .get_storage_logs_chunk(miniblock_number, hashed_keys_range)
            .await
            .context("Error fetching storage logs count")?;
        drop(conn);
        let latency = latency.observe();
        tracing::info!(
            "Loaded chunk {chunk_id} ({} logs) from Postgres in {latency:?}",
            logs.len()
        );

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
            .get_storage_prefix::<SnapshotStorageLogsChunk>();
        let output_filepath = format!("{output_filepath_prefix}/{filename}");
        let latency = latency.observe();

        let mut master_conn = self
            .master_pool
            .access_storage_tagged("snapshots_creator")
            .await?;
        master_conn
            .snapshots_dal()
            .add_storage_logs_filepath_for_snapshot(l1_batch_number, &output_filepath)
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

    async fn process_factory_deps(
        &self,
        miniblock_number: MiniblockNumber,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<String> {
        let mut conn = self
            .replica_pool
            .access_storage_tagged("snapshots_creator")
            .await?;

        tracing::info!("Loading factory deps from Postgres...");
        let latency =
            METRICS.factory_deps_processing_duration[&FactoryDepsStage::LoadFromPostgres].start();
        let factory_deps = conn
            .snapshots_creator_dal()
            .get_all_factory_deps(miniblock_number)
            .await?;
        drop(conn);
        let latency = latency.observe();
        tracing::info!("Loaded {} factory deps in {latency:?}", factory_deps.len());

        tracing::info!("Saving factory deps to GCS...");
        let latency =
            METRICS.factory_deps_processing_duration[&FactoryDepsStage::SaveToGcs].start();
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

    async fn run(self, config: SnapshotsCreatorConfig, min_chunk_count: u64) -> anyhow::Result<()> {
        let latency = METRICS.snapshot_generation_duration.start();

        let mut conn = self
            .replica_pool
            .access_storage_tagged("snapshots_creator")
            .await?;
        let mut master_conn = self
            .master_pool
            .access_storage_tagged("snapshots_creator")
            .await?;

        let latest_snapshot = master_conn
            .snapshots_dal()
            .get_newest_snapshot_metadata()
            .await?;
        let pending_snapshot = latest_snapshot
            .as_ref()
            .filter(|snapshot| !snapshot.is_complete());
        let progress = if let Some(snapshot) = pending_snapshot {
            SnapshotProgress::from_existing_snapshot(snapshot, &*self.blob_store)?
        } else {
            // We subtract 1 so that after restore, EN node has at least one L1 batch to fetch
            let sealed_l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await?;
            assert_ne!(
                sealed_l1_batch_number,
                L1BatchNumber(0),
                "Cannot create snapshot when only the genesis L1 batch is present in Postgres"
            );
            let l1_batch_number = sealed_l1_batch_number - 1;

            let latest_snapshot_l1_batch_number = latest_snapshot
                .as_ref()
                .map(|snapshot| snapshot.l1_batch_number);
            if latest_snapshot_l1_batch_number == Some(l1_batch_number) {
                tracing::info!(
                    "Snapshot at expected L1 batch #{l1_batch_number} is already created; exiting"
                );
                return Ok(());
            }

            let distinct_storage_logs_keys_count = conn
                .snapshots_creator_dal()
                .get_distinct_storage_logs_keys_count(l1_batch_number)
                .await?;
            let chunk_size = config.storage_logs_chunk_size;
            // We force the minimum number of chunks to avoid situations where only one chunk is created in tests.
            let chunk_count =
                ceil_div(distinct_storage_logs_keys_count, chunk_size).max(min_chunk_count);

            tracing::info!(
                "Selected storage logs chunking for L1 batch {l1_batch_number}: \
                {chunk_count} chunks of expected size {chunk_size}"
            );
            SnapshotProgress::new(l1_batch_number, chunk_count)
        };
        drop(master_conn);

        let (_, last_miniblock_number_in_batch) = conn
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(progress.l1_batch_number)
            .await?
            .context("Error fetching last miniblock number")?;
        drop(conn);

        METRICS.storage_logs_chunks_count.set(progress.chunk_count);
        tracing::info!(
            "Creating snapshot for storage logs up to miniblock {last_miniblock_number_in_batch}, \
            L1 batch {}",
            progress.l1_batch_number
        );

        if progress.needs_persisting_factory_deps {
            let factory_deps_output_file = self
                .process_factory_deps(last_miniblock_number_in_batch, progress.l1_batch_number)
                .await?;

            let mut master_conn = self
                .master_pool
                .access_storage_tagged("snapshots_creator")
                .await?;
            master_conn
                .snapshots_dal()
                .add_snapshot(
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
        let tasks = progress.remaining_chunk_ids.into_iter().map(|chunk_id| {
            self.process_storage_logs_single_chunk(
                &semaphore,
                last_miniblock_number_in_batch,
                progress.l1_batch_number,
                chunk_id,
                progress.chunk_count,
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

/// Minimum number of storage log chunks to produce.
const MIN_CHUNK_COUNT: u64 = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (stop_sender, stop_receiver) = watch::channel(false);

    tracing::info!("Starting snapshots creator");
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    maybe_enable_prometheus_metrics(stop_receiver).await?;
    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    let object_store_config =
        SnapshotsObjectStoreConfig::from_env().context("SnapshotsObjectStoreConfig::from_env()")?;
    let blob_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await;

    let postgres_config = PostgresConfig::from_env().context("PostgresConfig")?;
    let creator_config =
        SnapshotsCreatorConfig::from_env().context("SnapshotsCreatorConfig::from_env")?;

    let replica_pool = ConnectionPool::builder(
        postgres_config.replica_url()?,
        creator_config.concurrent_queries_count,
    )
    .build()
    .await?;

    let master_pool = ConnectionPool::singleton(postgres_config.master_url()?)
        .build()
        .await?;

    let config = SnapshotsCreatorConfig::from_env().context("SnapshotsCreatorConfig::from_env")?;
    let creator = SnapshotCreator {
        blob_store,
        master_pool,
        replica_pool,
        #[cfg(test)]
        event_listener: Box::new(()),
    };
    creator.run(config, MIN_CHUNK_COUNT).await?;

    tracing::info!("Finished running snapshot creator!");
    stop_sender.send(true).ok();
    Ok(())
}

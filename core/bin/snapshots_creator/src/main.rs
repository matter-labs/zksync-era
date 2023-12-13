//! Snapshot creator utility. Intended to run on a schedule, with each run creating a new snapshot.

use std::cmp::max;

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::{watch, Semaphore};
use zksync_config::{configs::PrometheusConfig, PostgresConfig, SnapshotsCreatorConfig};
use zksync_dal::ConnectionPool;
use zksync_env_config::{object_store::SnapshotsObjectStoreConfig, FromEnv};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    L1BatchNumber, MiniblockNumber,
};
use zksync_utils::ceil_div;

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

async fn process_storage_logs_single_chunk(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    semaphore: &Semaphore,
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
    chunk_id: u64,
    chunks_count: u64,
) -> anyhow::Result<String> {
    let latency =
        METRICS.storage_logs_processing_duration[&StorageChunkStage::AcquirePermit].start();
    let _permit = semaphore.acquire().await?;
    let latency = latency.observe();
    tracing::info!("Acquire permit for chunk {chunk_id} in {latency:?}");

    let hashed_keys_range = get_chunk_hashed_keys_range(chunk_id, chunks_count);
    let mut conn = pool.access_storage_tagged("snapshots_creator").await?;

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

    let latency = METRICS.storage_logs_processing_duration[&StorageChunkStage::SaveToGcs].start();
    let storage_logs_chunk = SnapshotStorageLogsChunk { storage_logs: logs };
    let key = SnapshotStorageLogsStorageKey {
        l1_batch_number,
        chunk_id,
    };
    let filename = blob_store
        .put(key, &storage_logs_chunk)
        .await
        .context("Error storing storage logs chunk in blob store")?;
    let output_filepath_prefix = blob_store.get_storage_prefix::<SnapshotStorageLogsChunk>();
    let output_filepath = format!("{output_filepath_prefix}/{filename}");
    let latency = latency.observe();

    let tasks_left = METRICS.storage_logs_chunks_left_to_process.dec_by(1) - 1;
    tracing::info!(
        "Saved chunk {chunk_id} to {output_filepath} in {latency:?}; overall progress {}/{chunks_count}",
        chunks_count - tasks_left
    );
    Ok(output_filepath)
}

async fn process_factory_deps(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<String> {
    let mut conn = pool.access_storage_tagged("snapshots_creator").await?;

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
    let latency = METRICS.factory_deps_processing_duration[&FactoryDepsStage::SaveToGcs].start();
    let factory_deps = SnapshotFactoryDependencies { factory_deps };
    let filename = blob_store
        .put(l1_batch_number, &factory_deps)
        .await
        .context("Error storing factory deps in blob store")?;
    let output_filepath_prefix = blob_store.get_storage_prefix::<SnapshotFactoryDependencies>();
    let output_filepath = format!("{output_filepath_prefix}/{filename}");
    let latency = latency.observe();
    tracing::info!(
        "Saved {} factory deps to {output_filepath} in {latency:?}",
        factory_deps.factory_deps.len()
    );

    Ok(output_filepath)
}

async fn run(
    blob_store: Box<dyn ObjectStore>,
    replica_pool: ConnectionPool,
    master_pool: ConnectionPool,
) -> anyhow::Result<()> {
    let latency = METRICS.snapshot_generation_duration.start();
    let config = SnapshotsCreatorConfig::from_env().context("SnapshotsCreatorConfig::from_env")?;

    let mut conn = replica_pool
        .access_storage_tagged("snapshots_creator")
        .await?;

    // We subtract 1 so that after restore, EN node has at least one L1 batch to fetch
    let sealed_l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await?;
    if sealed_l1_batch_number == L1BatchNumber(0) {
        tracing::info!("Node has no sealed L1 batches other than the genesis one; exiting");
        return Ok(());
    }
    let l1_batch_number = sealed_l1_batch_number - 1;

    let mut master_conn = master_pool
        .access_storage_tagged("snapshots_creator")
        .await?;
    if master_conn
        .snapshots_dal()
        .get_snapshot_metadata(l1_batch_number)
        .await?
        .is_some()
    {
        tracing::info!("Snapshot for L1 batch number {l1_batch_number} already exists, exiting");
        return Ok(());
    }
    drop(master_conn);

    let (_, last_miniblock_number_in_batch) = conn
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(l1_batch_number)
        .await?
        .context("Error fetching last miniblock number")?;
    let distinct_storage_logs_keys_count = conn
        .snapshots_creator_dal()
        .get_distinct_storage_logs_keys_count(l1_batch_number)
        .await?;
    drop(conn);

    let chunk_size = config.storage_logs_chunk_size;
    // we force at least 10 chunks to avoid situations where only one chunk is created in tests
    let chunks_count = max(10, ceil_div(distinct_storage_logs_keys_count, chunk_size));

    METRICS.storage_logs_chunks_count.set(chunks_count);

    tracing::info!(
        "Creating snapshot for storage logs up to miniblock {last_miniblock_number_in_batch}, \
        L1 batch {l1_batch_number}"
    );
    tracing::info!("Starting to generate {chunks_count} chunks of expected size {chunk_size}");

    let factory_deps_output_file = process_factory_deps(
        &*blob_store,
        &replica_pool,
        last_miniblock_number_in_batch,
        l1_batch_number,
    )
    .await?;

    METRICS
        .storage_logs_chunks_left_to_process
        .set(chunks_count);

    let semaphore = Semaphore::new(config.concurrent_queries_count as usize);
    let tasks = (0..chunks_count).map(|chunk_id| {
        process_storage_logs_single_chunk(
            &*blob_store,
            &replica_pool,
            &semaphore,
            last_miniblock_number_in_batch,
            l1_batch_number,
            chunk_id,
            chunks_count,
        )
    });
    let mut storage_logs_output_files = futures::future::try_join_all(tasks).await?;
    // Sanity check: the number of files should equal the number of chunks.
    assert_eq!(storage_logs_output_files.len(), chunks_count as usize);
    storage_logs_output_files.sort();

    tracing::info!("Finished generating snapshot, storing progress in Postgres");
    let mut master_conn = master_pool
        .access_storage_tagged("snapshots_creator")
        .await?;
    master_conn
        .snapshots_dal()
        .add_snapshot(
            l1_batch_number,
            &storage_logs_output_files,
            &factory_deps_output_file,
        )
        .await?;

    METRICS.snapshot_l1_batch.set(l1_batch_number.0 as u64);

    let elapsed = latency.observe();
    tracing::info!("snapshot_generation_duration: {elapsed:?}");
    tracing::info!("snapshot_l1_batch: {}", METRICS.snapshot_l1_batch.get());
    tracing::info!(
        "storage_logs_chunks_count: {}",
        METRICS.storage_logs_chunks_count.get()
    );
    Ok(())
}

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

    run(blob_store, replica_pool, master_pool).await?;
    tracing::info!("Finished running snapshot creator!");
    stop_sender.send(true).ok();
    Ok(())
}

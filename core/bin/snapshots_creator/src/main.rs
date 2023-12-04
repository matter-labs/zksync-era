mod chunking;

use anyhow::Context as _;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use prometheus_exporter::PrometheusExporterConfig;
use std::cmp::max;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use vise::{Buckets, Gauge, Histogram, Metrics};
use zksync_config::configs::PrometheusConfig;
use zksync_config::{PostgresConfig, SnapshotsCreatorConfig};

use crate::chunking::get_chunk_hashed_keys_range;
use zksync_dal::ConnectionPool;
use zksync_env_config::object_store::SnapshotsObjectStoreConfig;
use zksync_env_config::FromEnv;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::snapshots::{
    SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
};
use zksync_types::zkevm_test_harness::zk_evm::zkevm_opcode_defs::decoding::AllowedPcOrImm;
use zksync_types::{L1BatchNumber, MiniblockNumber};
use zksync_utils::ceil_div;
use zksync_utils::time::seconds_since_epoch;

#[derive(Debug, Metrics)]
#[metrics(prefix = "snapshots_creator")]
struct SnapshotsCreatorMetrics {
    storage_logs_chunks_count: Gauge<u64>,

    storage_logs_chunks_left_to_process: Gauge<u64>,

    snapshot_generation_duration: Gauge<u64>,

    snapshot_l1_batch: Gauge<u64>,

    #[metrics(buckets = Buckets::LATENCIES)]
    storage_logs_processing_durations: Histogram<Duration>,

    #[metrics(buckets = Buckets::LATENCIES)]
    factory_deps_processing_durations: Histogram<Duration>,
}
#[vise::register]
pub(crate) static METRICS: vise::Global<SnapshotsCreatorMetrics> = vise::Global::new();

async fn maybe_enable_prometheus_metrics(stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
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
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
    chunk_id: u64,
    chunks_count: u64,
) -> anyhow::Result<String> {
    let (min_hashed_key, max_hashed_key) = get_chunk_hashed_keys_range(chunk_id, chunks_count);
    let latency = METRICS.storage_logs_processing_durations.start();
    let mut conn = pool.access_storage_tagged("snapshots_creator").await?;
    let logs = conn
        .snapshots_creator_dal()
        .get_storage_logs_chunk(miniblock_number, &min_hashed_key, &max_hashed_key)
        .await
        .context("Error fetching storage logs count")?;
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

    let elapsed_ms = latency.observe().as_millis();
    tracing::info!(
        "Finished storage logs chunk {}/{chunks_count}, step took {elapsed_ms}ms, output stored in {output_filepath}",
        chunk_id + 1
    );
    drop(conn);
    Ok(output_filepath)
}

async fn process_factory_deps(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<String> {
    let latency = METRICS.factory_deps_processing_durations.start();
    let mut conn = pool.access_storage_tagged("snapshots_creator").await?;
    let factory_deps = conn
        .snapshots_creator_dal()
        .get_all_factory_deps(miniblock_number)
        .await?;
    let factory_deps = SnapshotFactoryDependencies { factory_deps };
    drop(conn);
    let filename = blob_store
        .put(l1_batch_number, &factory_deps)
        .await
        .context("Error storing factory deps in blob store")?;
    let output_filepath_prefix = blob_store.get_storage_prefix::<SnapshotFactoryDependencies>();
    let output_filepath = format!("{output_filepath_prefix}/{filename}");
    let elapsed_ms = latency.observe().as_millis();
    tracing::info!(
        "Finished factory dependencies, step took {elapsed_ms}ms , output stored in {}",
        output_filepath
    );
    Ok(output_filepath)
}

async fn run(
    blob_store: Box<dyn ObjectStore>,
    replica_pool: ConnectionPool,
    master_pool: ConnectionPool,
) -> anyhow::Result<()> {
    let config = SnapshotsCreatorConfig::from_env().context("SnapshotsCreatorConfig::from_env")?;

    let mut conn = replica_pool
        .access_storage_tagged("snapshots_creator")
        .await?;
    let start_time = seconds_since_epoch();

    // we subtract 1 so that after restore, EN node has at least one l1 batch to fetch
    let l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await? - 1;

    let mut master_conn = master_pool
        .access_storage_tagged("snapshots_creator")
        .await?;
    if master_conn
        .snapshots_dal()
        .get_snapshot_metadata(l1_batch_number)
        .await?
        .is_some()
    {
        tracing::info!("Snapshot for L1 batch number {l1_batch_number} already exists, exiting",);
        return Ok(());
    }
    drop(master_conn);

    // snapshots always
    let last_miniblock_number_in_batch = conn
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(l1_batch_number)
        .await?
        .context("Error fetching last miniblock number")?
        .1;
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
        "Creating snapshot for storage logs up to miniblock {last_miniblock_number_in_batch}, l1_batch {}",
        l1_batch_number.0
    );
    tracing::info!("Starting to generate {chunks_count} chunks of expected size {chunk_size}");

    let factory_deps_output_file = process_factory_deps(
        &*blob_store,
        &replica_pool,
        last_miniblock_number_in_batch,
        l1_batch_number,
    )
    .await?;

    let mut storage_logs_output_files = vec![];

    METRICS
        .storage_logs_chunks_left_to_process
        .set(chunks_count);
    let mut tasks =
        FuturesUnordered::<Pin<Box<dyn Future<Output = anyhow::Result<String>>>>>::new();
    let mut last_chunk_id = 0;
    while last_chunk_id < chunks_count || tasks.len() != 0 {
        while (tasks.len() as u32) < config.concurrent_queries_count && last_chunk_id < chunks_count
        {
            tasks.push(Box::pin(process_storage_logs_single_chunk(
                &*blob_store,
                &replica_pool,
                last_miniblock_number_in_batch,
                l1_batch_number,
                last_chunk_id,
                chunks_count,
            )));
            last_chunk_id += 1;
        }
        if let Some(result) = tasks.next().await {
            tracing::info!(
                "Completed chunk {}/{}, {} chunks are still in progress",
                last_chunk_id - tasks.len() as u64,
                chunks_count,
                tasks.len()
            );
            storage_logs_output_files.push(result.unwrap());
            METRICS
                .storage_logs_chunks_left_to_process
                .set(chunks_count - last_chunk_id - tasks.len() as u64);
        }
    }
    tracing::info!("Finished generating snapshot, storing progress in db");

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

    METRICS.snapshot_l1_batch.set(l1_batch_number.0.as_u64());
    METRICS
        .snapshot_generation_duration
        .set(seconds_since_epoch() - start_time);

    tracing::info!("Run metrics:");
    tracing::info!(
        "snapshot_generation_duration: {}s",
        METRICS.snapshot_generation_duration.get()
    );
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

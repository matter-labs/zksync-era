use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use vise::{Gauge, Metrics};
use zksync_config::configs::PrometheusConfig;
use zksync_config::PostgresConfig;

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
    pub storage_logs_chunks_count: Gauge<u64>,

    pub snapshot_generation_duration: Gauge<u64>,

    pub snapshot_l1_batch: Gauge<u64>,

    pub snapshot_generation_timestamp: Gauge<u64>,
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
    l1_batch_number: L1BatchNumber,
    chunk_id: u64,
    chunk_size: u64,
    chunks_count: u64,
) -> anyhow::Result<String> {
    let mut conn = pool.access_storage().await?;
    let logs = conn
        .snapshots_creator_dal()
        .get_storage_logs_chunk(l1_batch_number, chunk_id, chunk_size)
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

    tracing::info!(
        "Finished storing storage logs chunk {}/{chunks_count}, output stored in {output_filepath}",
        chunk_id + 1,
    );
    Ok(output_filepath)
}

async fn process_factory_deps(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<String> {
    tracing::info!("Processing factory dependencies");
    let mut conn = pool.access_storage().await?;
    let factory_deps = conn
        .snapshots_creator_dal()
        .get_all_factory_deps(miniblock_number)
        .await;
    let factory_deps = SnapshotFactoryDependencies { factory_deps };
    let filename = blob_store
        .put(l1_batch_number, &factory_deps)
        .await
        .context("Error storing factory deps in blob store")?;
    let output_filepath_prefix = blob_store.get_storage_prefix::<SnapshotFactoryDependencies>();
    let output_filepath = format!("{output_filepath_prefix}/{filename}");
    tracing::info!(
        "Finished processing factory dependencies, output stored in {}",
        output_filepath
    );
    Ok(output_filepath)
}

async fn run(blob_store: Box<dyn ObjectStore>, pool: ConnectionPool) -> anyhow::Result<()> {
    let mut conn = pool.access_storage().await?;
    let start_time = seconds_since_epoch();

    let l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await? - 1; // we subtract 1 so that after restore, EN node has at least one l1 batch to fetch

    if conn
        .snapshots_dal()
        .get_snapshot_metadata(l1_batch_number)
        .await?
        .is_some()
    {
        tracing::info!(
            "Snapshot for L1 batch number {} already exists, exiting",
            l1_batch_number
        );
        return Ok(());
    }

    let miniblock_number = conn
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(l1_batch_number)
        .await?
        .unwrap()
        .1;
    let storage_logs_count = conn
        .snapshots_creator_dal()
        .get_storage_logs_count(l1_batch_number)
        .await?;

    drop(conn);

    //TODO load this from config
    let chunk_size = 1_000_000;
    let chunks_count = ceil_div(storage_logs_count, chunk_size);

    tracing::info!(
        "Creating snapshot for storage logs up to miniblock {}, l1_batch {}",
        miniblock_number,
        l1_batch_number.0
    );
    tracing::info!(
        "Starting to generate {} chunks of max size {}",
        chunks_count,
        chunk_size
    );

    let factory_deps_output_file =
        process_factory_deps(&*blob_store, &pool, miniblock_number, l1_batch_number).await?;

    let mut storage_logs_output_files = vec![];

    for chunk_id in 0..chunks_count {
        tracing::info!(
            "Processing storage logs chunk {}/{chunks_count}",
            chunk_id + 1
        );
        let output_file = process_storage_logs_single_chunk(
            &*blob_store,
            &pool,
            l1_batch_number,
            chunk_id,
            chunk_size,
            chunks_count,
        )
        .await?;
        storage_logs_output_files.push(output_file.clone());
    }

    let mut conn = pool.access_storage().await?;

    conn.snapshots_dal()
        .add_snapshot(
            l1_batch_number,
            &storage_logs_output_files,
            factory_deps_output_file,
        )
        .await?;

    METRICS.snapshot_l1_batch.set(l1_batch_number.0.as_u64());
    METRICS.storage_logs_chunks_count.set(chunks_count);
    METRICS
        .snapshot_generation_timestamp
        .set(seconds_since_epoch());
    METRICS
        .snapshot_generation_duration
        .set(seconds_since_epoch() - start_time);

    tracing::info!(
        r#"Run metrics:
snapshot_generation_duration: {}sec
snapshot_l1_batch: {},
snapshot_generation_timestamp: {}
storage_logs_chunks_count: {}
  "#,
        METRICS.snapshot_generation_duration.get(),
        METRICS.snapshot_l1_batch.get(),
        METRICS.snapshot_generation_timestamp.get(),
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
    let pool = ConnectionPool::singleton(postgres_config.replica_url()?)
        .build()
        .await?;

    run(blob_store, pool).await?;
    tracing::info!("Finished running snapshot creator!");
    stop_sender.send(true).ok();
    Ok(())
}

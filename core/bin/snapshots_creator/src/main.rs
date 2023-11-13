use anyhow::Context as _;

use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_env_config::object_store::SnapshotsObjectStoreConfig;
use zksync_env_config::FromEnv;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::snapshots::{
    SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
};
use zksync_types::{L1BatchNumber, MiniblockNumber};
use zksync_utils::ceil_div;

async fn process_storage_logs_single_chunk(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    l1_batch_number: L1BatchNumber,
    chunk_id: u64,
    chunk_size: u64,
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
    blob_store
        .put(key, &storage_logs_chunk)
        .await
        .context("Error storing storage logs chunk in blob store")?;

    let output_file = blob_store.get_full_path::<SnapshotStorageLogsChunk>(key);

    Ok(output_file)
}

async fn process_factory_deps(
    blob_store: &dyn ObjectStore,
    pool: &ConnectionPool,
    miniblock_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
) -> anyhow::Result<String> {
    let mut conn = pool.access_storage().await?;
    let factory_deps = conn
        .snapshots_creator_dal()
        .get_all_factory_deps(miniblock_number)
        .await;
    let factory_deps = SnapshotFactoryDependencies { factory_deps };
    blob_store
        .put(l1_batch_number, &factory_deps)
        .await
        .context("Error storing factory deps in blob store")?;
    let factory_deps_output_file =
        blob_store.get_full_path::<SnapshotFactoryDependencies>(l1_batch_number);
    Ok(factory_deps_output_file)
}

async fn run(blob_store: Box<dyn ObjectStore>, pool: ConnectionPool) -> anyhow::Result<()> {
    // TODO metrics
    let mut conn = pool.access_storage().await?;

    let l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await? - 1; // we subtract 1 so that after restore, EN node has at least one l1 batch to fetch

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

    //TODO load this from config
    let chunk_size = 1_000_000;
    let chunks_count = ceil_div(storage_logs_count, chunk_size);

    tracing::info!(
        "Creating snapshot for storage logs up to miniblock {}, l1_batch {}",
        miniblock_number,
        l1_batch_number.0
    );
    tracing::info!(
        "{} chunks of max size {} will be generated",
        chunks_count,
        chunk_size
    );

    let factory_deps_output_file =
        process_factory_deps(&*blob_store, &pool, miniblock_number, l1_batch_number).await?;

    let mut storage_logs_output_files = vec![];

    for chunk_id in 0..chunks_count {
        let output_file = process_storage_logs_single_chunk(
            &*blob_store,
            &pool,
            l1_batch_number,
            chunk_id,
            chunk_size,
        )
        .await?;
        storage_logs_output_files.push(output_file.clone());
        tracing::info!(
            "Finished storing chunk {}/{chunks_count} in {output_file}",
            chunk_id + 1,
        );
    }

    conn.snapshots_dal()
        .add_snapshot(
            l1_batch_number,
            &storage_logs_output_files,
            factory_deps_output_file,
        )
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing::info!("Starting snapshots creator");
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

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

    let pool = ConnectionPool::builder(DbVariant::Master)
        .build()
        .await
        .unwrap();

    run(blob_store, pool).await?;
    tracing::info!("Finished running snapshot creator!");
    Ok(())
}

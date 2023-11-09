use anyhow::Context as _;

use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::snapshots::{
    SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
};
use zksync_utils::ceil_div;

async fn run(blob_store: Box<dyn ObjectStore>, pool: ConnectionPool) {
    // TODO metrics
    let mut conn = pool.access_storage().await.unwrap();

    let l1_batch_number = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        - 1; // we subtract 1 so that after restore, EN node has at least one l1 batch to fetch

    let miniblock_number = conn
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(l1_batch_number)
        .await
        .unwrap()
        .unwrap()
        .1;
    let storage_logs_count = conn
        .storage_logs_snapshots_dal()
        .get_storage_logs_count(l1_batch_number)
        .await
        .unwrap();

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

    let mut output_files = vec![];

    let factory_deps = conn
        .storage_logs_snapshots_dal()
        .get_all_factory_deps(miniblock_number)
        .await;
    let factory_deps = SnapshotFactoryDependencies { factory_deps };
    blob_store
        .put(l1_batch_number, &factory_deps)
        .await
        .unwrap();
    let factory_deps_output_file =
        blob_store.get_full_path::<SnapshotFactoryDependencies>(l1_batch_number);

    for chunk_id in 0..chunks_count {
        let logs = conn
            .storage_logs_snapshots_dal()
            .get_storage_logs_chunk(l1_batch_number, chunk_id, chunk_size)
            .await
            .unwrap();
        let storage_logs_chunk = SnapshotStorageLogsChunk { storage_logs: logs };
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number,
            chunk_id,
        };
        blob_store.put(key, &storage_logs_chunk).await.unwrap();
        let output_file = blob_store.get_full_path::<SnapshotStorageLogsChunk>(key);
        output_files.push(output_file.clone());
        tracing::info!(
            "Finished storing chunk {}/{} in {}",
            key.chunk_id + 1,
            chunks_count,
            output_file
        );
    }

    conn.snapshots_dal()
        .add_snapshot(l1_batch_number, &output_files, factory_deps_output_file)
        .await
        .unwrap();
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

    let blob_store = ObjectStoreFactory::snapshots_from_env()
        .context("ObjectStoreFactor::snapshots_from_env()")?
        .create_store()
        .await;

    let pool = ConnectionPool::builder(DbVariant::Master)
        .build()
        .await
        .unwrap();

    run(blob_store, pool).await;
    tracing::info!("Finished running snapshot creator!");
    Ok(())
}

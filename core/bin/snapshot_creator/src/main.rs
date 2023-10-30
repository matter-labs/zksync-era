use anyhow::Context as _;

use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::snapshots::{FactoryDependency, SnapshotChunk, SnapshotStorageKey};
use zksync_utils::ceil_div;

async fn run(blob_store: Box<dyn ObjectStore>, pool: ConnectionPool) {
    // TODO metrics
    let mut conn = pool.access_storage().await.unwrap();

    let batch_id = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap();
    let miniblock_number = conn
        .storage_logs_snapshots_dal()
        .get_last_miniblock_number(batch_id)
        .await
        .unwrap();
    let logs_count = conn
        .storage_logs_snapshots_dal()
        .get_storage_logs_count(batch_id)
        .await
        .unwrap();

    //TODO load this from config
    let chunk_size = 1_000_000;
    let chunks_count = ceil_div(logs_count, chunk_size);

    println!(
        "Creating snapshot for storage logs up to miniblock {}, l1_batch {}",
        miniblock_number, batch_id.0
    );
    println!(
        "{} chunks of max size {} will be generated",
        chunks_count, chunk_size
    );

    let mut output_files = vec![];
    for chunk_id in 0..chunks_count {
        let logs = conn
            .storage_logs_snapshots_dal()
            .get_storage_logs_chunk(batch_id, chunk_id, chunk_size)
            .await
            .unwrap();
        let mut factory_deps: Vec<FactoryDependency> = vec![];
        if chunk_id == 0 {
            factory_deps = conn
                .storage_logs_snapshots_dal()
                .get_all_factory_deps(miniblock_number)
                .await;
        }
        let result = SnapshotChunk {
            storage_logs: logs,
            factory_deps,
        };
        let key = SnapshotStorageKey {
            l1_batch_number: batch_id,
            chunk_id,
        };
        let result = blob_store.put(key, &result).await;
        let output_file = result.unwrap();
        output_files.push(output_file.clone());
        println!(
            "Finished storing chunk {}/{} in {}",
            key.chunk_id + 1,
            chunks_count,
            output_file
        );
    }

    conn.snapshots_dal()
        .add_snapshot(batch_id, miniblock_number, &output_files)
        .await
        .unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Running creator");
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

    let blob_store = ObjectStoreFactory::prover_from_env()
        .context("ObjectStoreFactor::prover_from_env()")?
        .create_store()
        .await;

    let pool = ConnectionPool::builder(DbVariant::Master)
        .build()
        .await
        .unwrap();

    run(blob_store, pool).await;
    println!("Finished!");
    Ok(())
}

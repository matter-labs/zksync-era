use anyhow::Context as _;

use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory, StoredObject};
use zksync_types::snapshots::{StorageLogsSnapshot, StorageLogsSnapshotKey};
use zksync_types::L1BatchNumber;

async fn run(blob_store: Box<dyn ObjectStore>, pool: ConnectionPool) {
    // TODO metrics
    let mut conn = pool.access_storage().await.unwrap();
    // TODO actual logic and storage logs retrieval
    let fake_batch_id = L1BatchNumber(
        conn.snapshots_dal()
            .get_snapshots()
            .await
            .unwrap()
            .snapshots
            .len() as u32
            + 1,
    );
    print!("Creating snapshot for block {}\n", fake_batch_id.0);
    let result = StorageLogsSnapshot {
        l1_batch_number: fake_batch_id,
    };
    let key = StorageLogsSnapshotKey {
        l1_batch_number: fake_batch_id,
        chunk_id: 123,
    };
    let result = blob_store.put(key, &result).await;

    let output_filename = result.unwrap();
    let files = [output_filename];
    conn.snapshots_dal()
        .add_snapshot(fake_batch_id, &files)
        .await
        .unwrap();
    print!("Stored chunk {} in {} \n", key.chunk_id, files[0]);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print!("Running creator\n");
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
    print!("Finished!\n");
    Ok(())
}

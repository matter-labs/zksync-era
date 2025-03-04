//! Object store manual connection test.

use anyhow::Context as _;
use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use zksync_core_leftovers::temp_config_store::load_general_config;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    snapshots::{SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey},
    L1BatchNumber, H256,
};

/// CLI for testing Object Storage using core_object_store config field.
#[derive(Debug, Parser)]
struct Cli {
    /// Number of updates to perform.
    #[arg(name = "config", alias = "c")]
    config_path: Option<std::path::PathBuf>,
}

impl Cli {
    fn init_logging() {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy()
                    .add_directive("zksync_object_store=trace".parse().unwrap()),
            )
            .init();
    }

    #[tokio::main]
    async fn run(self) -> anyhow::Result<()> {
        Self::init_logging();
        tracing::info!("Launched with options: {self:?}");
        let general_config = load_general_config(self.config_path).context("general config")?;

        let object_store_config = general_config
            .core_object_store
            .context("core object store config")?;
        let object_store = ObjectStoreFactory::new(object_store_config)
            .create_store()
            .await
            .context("failed to create object store")?;

        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(123456),
            chunk_id: 4321,
        };
        let snapshot_chunk = SnapshotStorageLogsChunk::<H256> {
            storage_logs: vec![],
        };

        object_store
            .put::<SnapshotStorageLogsChunk>(key, &snapshot_chunk)
            .await?;
        let result = object_store.get::<SnapshotStorageLogsChunk>(key).await?;
        tracing::info!("Result: {result:?}");
        object_store.remove::<SnapshotStorageLogsChunk>(key).await?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    Cli::parse().run()
}

//! Object store manual connection test.

use anyhow::Context as _;
use clap::Parser;
use smart_config::{ConfigRepository, ConfigSchema, DescribeConfig, Environment};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use zksync_config::{sources::ConfigFilePaths, ObjectStoreConfig};
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

        let config_schema = ConfigSchema::new(&ObjectStoreConfig::DESCRIPTION, "core_object_store");
        let mut config_repo = ConfigRepository::new(&config_schema).with(Environment::prefixed(""));
        if let Some(path) = &self.config_path {
            let yaml = ConfigFilePaths::read_yaml(path)?;
            config_repo = config_repo.with(yaml);
        }
        config_repo.deserializer_options().coerce_variant_names = true;
        let mut config_repo = zksync_config::ConfigRepository::from(config_repo);

        let object_store_config: ObjectStoreConfig = config_repo.parse()?;
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

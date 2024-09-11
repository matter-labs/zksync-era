use std::{path::Path, time::Instant};

use anyhow::Context as _;
use clap::Parser;
use zksync_config::{configs::ObservabilityConfig, DBConfig};
use zksync_env_config::FromEnv;
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    version,
    about = "Merkle tree consistency checker",
    long_about = None
)]
struct Cli {
    /// Specifies the version of the tree to be checked, expressed as a 0-based L1 batch number
    /// applied to it last. If not specified, the latest tree version is checked.
    #[arg(long = "l1-batch")]
    l1_batch: Option<u32>,
}

impl Cli {
    fn run(self, config: &DBConfig) -> anyhow::Result<()> {
        let db_path = &config.merkle_tree.path;
        tracing::info!("Verifying consistency of Merkle tree at {db_path}");
        let start = Instant::now();
        let db =
            RocksDB::new(Path::new(db_path)).context("failed initializing Merkle tree RocksDB")?;
        let tree =
            ZkSyncTree::new_lightweight(db.into()).context("cannot initialize Merkle tree")?;

        let l1_batch_number = if let Some(number) = self.l1_batch {
            L1BatchNumber(number)
        } else {
            let next_number = tree.next_l1_batch_number();
            if next_number == L1BatchNumber(0) {
                tracing::info!("Merkle tree is empty, skipping");
                return Ok(());
            }
            next_number - 1
        };

        tracing::info!("L1 batch number to check: {l1_batch_number}");
        tree.verify_consistency(l1_batch_number)
            .context("Merkle tree is inconsistent")?;
        tracing::info!("Merkle tree verified in {:?}", start.elapsed());
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let _observability_guard = observability_config.install()?;

    let db_config = DBConfig::from_env().context("DBConfig::from_env()")?;
    Cli::parse().run(&db_config)
}

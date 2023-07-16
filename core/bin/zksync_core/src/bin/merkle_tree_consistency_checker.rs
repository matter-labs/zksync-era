use clap::Parser;

use std::{num::NonZeroU32, time::Instant};

use zksync_config::DBConfig;
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_storage::RocksDB;

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    version,
    about = "Merkle tree consistency checker",
    long_about = None
)]
struct Cli {
    /// Specifies the version of the tree to be checked, expressed as a non-zero number
    /// of blocks applied to it. By default, the latest tree version is checked.
    #[arg(long = "blocks")]
    blocks: Option<NonZeroU32>,
}

impl Cli {
    fn run(self, config: &DBConfig) {
        let db_path = &config.new_merkle_tree_ssd_path;
        vlog::info!("Verifying consistency of Merkle tree at {db_path}");
        let start = Instant::now();
        let db = RocksDB::new(db_path, true);
        let tree = ZkSyncTree::new_lightweight(db);

        let block_number = self.blocks.or_else(|| NonZeroU32::new(tree.block_number()));
        if let Some(block_number) = block_number {
            vlog::info!("Block number to check: {block_number}");
            tree.verify_consistency(block_number);
            vlog::info!("Merkle tree verified in {:?}", start.elapsed());
        } else {
            vlog::info!("Merkle tree is empty, skipping");
        }
    }
}

fn main() {
    vlog::init();
    let _sentry_guard = vlog::init_sentry();
    let db_config = DBConfig::from_env();
    Cli::parse().run(&db_config);
}

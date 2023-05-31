use clap::Parser;

use std::{num::NonZeroU32, time::Instant};

use zksync_config::ZkSyncConfig;
use zksync_merkle_tree::ZkSyncTree;
use zksync_merkle_tree2::domain::ZkSyncTree as NewTree;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    version,
    about = "Merkle tree consistency checker",
    long_about = None
)]
struct Cli {
    /// Do not check the old tree implementation in full mode. By default, this is the only
    /// tree checked.
    #[arg(long = "no-full")]
    no_full: bool,
    /// Check the old tree implementation in lightweight mode.
    #[arg(long = "lightweight")]
    lightweight: bool,
    /// Check the new tree implementation in lightweight mode. The optional argument
    /// specifies the version of the tree to be checked, expressed as a non-zero number
    /// of blocks applied to it. By default, the latest tree version is checked.
    #[arg(long = "lightweight-new", value_name = "BLOCKS")]
    new_lightweight: Option<Option<NonZeroU32>>,
}

impl Cli {
    fn run(self, config: &ZkSyncConfig) {
        if !self.no_full {
            let db_path = config.db.path();
            vlog::info!(
                "Verifying consistency of old tree, full mode at {}",
                db_path
            );
            let start = Instant::now();
            let db = RocksDB::new(Database::MerkleTree, db_path, true);
            let tree = ZkSyncTree::new(db);
            tree.verify_consistency();
            vlog::info!("Old tree in full mode verified in {:?}", start.elapsed());
        }

        if self.lightweight {
            let db_path = config.db.merkle_tree_fast_ssd_path();
            vlog::info!(
                "Verifying consistency of old tree, lightweight mode at {}",
                db_path
            );
            let start = Instant::now();
            let db = RocksDB::new(Database::MerkleTree, db_path, true);
            let tree = ZkSyncTree::new_lightweight(db);
            tree.verify_consistency();
            vlog::info!(
                "Old tree in lightweight mode verified in {:?}",
                start.elapsed()
            );
        }

        if let Some(maybe_block_number) = self.new_lightweight {
            let db_path = &config.db.new_merkle_tree_ssd_path;
            vlog::info!(
                "Verifying consistency of new tree, lightweight mode at {}",
                db_path
            );
            let start = Instant::now();
            let db = RocksDB::new(Database::MerkleTree, db_path, true);
            let tree = NewTree::new_lightweight(db);

            let block_number = maybe_block_number.or_else(|| NonZeroU32::new(tree.block_number()));
            if let Some(block_number) = block_number {
                vlog::info!("Block number to check: {}", block_number);
                tree.verify_consistency(block_number);
                vlog::info!(
                    "New tree in lightweight mode verified in {:?}",
                    start.elapsed()
                );
            } else {
                vlog::info!("The tree is empty, skipping");
            }
        }
    }
}

fn main() {
    let _sentry_guard = vlog::init();
    let config = ZkSyncConfig::from_env();
    Cli::parse().run(&config);
}

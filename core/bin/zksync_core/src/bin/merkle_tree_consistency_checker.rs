use clap::Parser;

use std::time::Instant;

use zksync_config::DBConfig;
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
    fn run(self, config: &DBConfig) {
        let db_path = &config.merkle_tree.path;
        vlog::info!("Verifying consistency of Merkle tree at {db_path}");
        let start = Instant::now();
        let db = RocksDB::new(db_path, true);
        let tree = ZkSyncTree::new_lightweight(db);

        let l1_batch_number = if let Some(number) = self.l1_batch {
            L1BatchNumber(number)
        } else {
            let next_number = tree.next_l1_batch_number();
            if next_number == L1BatchNumber(0) {
                vlog::info!("Merkle tree is empty, skipping");
                return;
            }
            next_number - 1
        };

        vlog::info!("L1 batch number to check: {l1_batch_number}");
        tree.verify_consistency(l1_batch_number);
        vlog::info!("Merkle tree verified in {:?}", start.elapsed());
    }
}

fn main() {
    vlog::init();
    let _sentry_guard = vlog::init_sentry();
    let db_config = DBConfig::from_env();
    Cli::parse().run(&db_config);
}

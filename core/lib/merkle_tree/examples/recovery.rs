//! Tree recovery load test.

use clap::Parser;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tempfile::TempDir;

use std::time::Instant;

use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    recovery::{MerkleTreeRecovery, RecoveryEntry},
    Database, HashTree, Key, PatchSet, RocksDBWrapper, ValueHash,
};
use zksync_storage::RocksDB;

/// CLI for load-testing Merkle tree recovery.
#[derive(Debug, Parser)]
struct Cli {
    /// Number of updates to perform.
    #[arg(name = "updates")]
    update_count: u64,
    /// Number of entries per update.
    #[arg(name = "ops")]
    writes_per_update: usize,
    /// Use a no-op hashing function.
    #[arg(name = "no-hash", long)]
    no_hashing: bool,
    /// Perform testing on in-memory DB rather than RocksDB (i.e., with focus on hashing logic).
    #[arg(long = "in-memory", short = 'M')]
    in_memory: bool,
    /// Block cache capacity for RocksDB in bytes.
    #[arg(long = "block-cache", conflicts_with = "in_memory")]
    block_cache: Option<usize>,
    /// Seed to use in the RNG for reproducibility.
    #[arg(long = "rng-seed", default_value = "0")]
    rng_seed: u64,
}

impl Cli {
    fn run(self) {
        println!("Launched with options: {self:?}");

        let (mut mock_db, mut rocksdb);
        let mut _temp_dir = None;
        let db: &mut dyn Database = if self.in_memory {
            mock_db = PatchSet::default();
            &mut mock_db
        } else {
            let dir = TempDir::new().expect("failed creating temp dir for RocksDB");
            println!(
                "Created temp dir for RocksDB: {}",
                dir.path().to_string_lossy()
            );
            rocksdb = if let Some(block_cache_capacity) = self.block_cache {
                let db = RocksDB::with_cache(dir.path(), true, Some(block_cache_capacity));
                RocksDBWrapper::from(db)
            } else {
                RocksDBWrapper::new(dir.path())
            };
            _temp_dir = Some(dir);
            &mut rocksdb
        };

        let hasher: &dyn HashTree = if self.no_hashing { &() } else { &Blake2Hasher };
        let mut rng = StdRng::seed_from_u64(self.rng_seed);

        let recovered_version = 123;
        let mut last_key = Key::zero();
        let mut last_leaf_index = 0;
        let mut recovery = MerkleTreeRecovery::with_hasher(db, recovered_version, hasher);
        let recovery_started_at = Instant::now();
        for updated_idx in 0..self.update_count {
            let started_at = Instant::now();
            let recovery_entries = (0..self.writes_per_update)
                .map(|_| {
                    last_key += Key::from(rng.gen::<u64>());
                    last_leaf_index += 1;
                    RecoveryEntry {
                        key: last_key,
                        value: ValueHash::zero(),
                        leaf_index: last_leaf_index,
                    }
                })
                .collect();
            recovery.extend(recovery_entries);
            println!(
                "Updated tree with recovery chunk #{updated_idx} in {:?}",
                started_at.elapsed()
            );
        }

        let tree = recovery.finalize();
        println!(
            "Recovery finished in {:?}; verifying consistency...",
            recovery_started_at.elapsed()
        );
        let started_at = Instant::now();
        tree.verify_consistency(recovered_version).unwrap();
        println!("Verified consistency in {:?}", started_at.elapsed());
    }
}

fn main() {
    Cli::parse().run();
}

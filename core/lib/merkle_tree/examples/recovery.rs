//! Tree recovery load test.

use clap::Parser;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;

use std::time::Instant;

use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    recovery::{MerkleTreeRecovery, RecoveryEntry},
    HashTree, Key, PatchSet, PruneDatabase, RocksDBWrapper, ValueHash,
};
use zksync_storage::{RocksDB, RocksDBOptions};

/// CLI for load-testing Merkle tree recovery.
#[derive(Debug, Parser)]
struct Cli {
    /// Number of updates to perform.
    #[arg(name = "updates")]
    update_count: u64,
    /// Number of entries per update.
    #[arg(name = "ops")]
    writes_per_update: usize,
    /// Perform random recovery instead of linear recovery.
    #[arg(name = "random", long)]
    random: bool,
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
    fn init_logging() {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    fn run(self) {
        Self::init_logging();
        tracing::info!("Launched with options: {self:?}");

        let (mut mock_db, mut rocksdb);
        let mut _temp_dir = None;
        let db: &mut dyn PruneDatabase = if self.in_memory {
            mock_db = PatchSet::default();
            &mut mock_db
        } else {
            let dir = TempDir::new().expect("failed creating temp dir for RocksDB");
            tracing::info!(
                "Created temp dir for RocksDB: {}",
                dir.path().to_string_lossy()
            );
            let db = RocksDB::with_options(
                dir.path(),
                RocksDBOptions {
                    block_cache_capacity: self.block_cache,
                    ..RocksDBOptions::default()
                },
            );
            rocksdb = RocksDBWrapper::from(db);
            _temp_dir = Some(dir);
            &mut rocksdb
        };

        let hasher: &dyn HashTree = if self.no_hashing { &() } else { &Blake2Hasher };
        let mut rng = StdRng::seed_from_u64(self.rng_seed);

        let recovered_version = 123;
        let key_step =
            Key::MAX / (Key::from(self.update_count) * Key::from(self.writes_per_update));
        assert!(key_step > Key::from(u64::MAX));
        // ^ Total number of generated keys is <2^128.

        let mut last_key = Key::zero();
        let mut last_leaf_index = 0;
        let mut recovery = MerkleTreeRecovery::with_hasher(db, recovered_version, hasher);
        let recovery_started_at = Instant::now();
        for updated_idx in 0..self.update_count {
            let started_at = Instant::now();
            let recovery_entries = (0..self.writes_per_update)
                .map(|_| {
                    last_leaf_index += 1;
                    if self.random {
                        RecoveryEntry {
                            key: Key::from(rng.gen::<[u8; 32]>()),
                            value: ValueHash::zero(),
                            leaf_index: last_leaf_index,
                        }
                    } else {
                        last_key += key_step - Key::from(rng.gen::<u64>());
                        // ^ Increases the key by a random increment close to `key` step with some randomness.
                        RecoveryEntry {
                            key: last_key,
                            value: ValueHash::zero(),
                            leaf_index: last_leaf_index,
                        }
                    }
                })
                .collect();
            if self.random {
                recovery.extend_random(recovery_entries);
            } else {
                recovery.extend_linear(recovery_entries);
            }
            tracing::info!(
                "Updated tree with recovery chunk #{updated_idx} in {:?}",
                started_at.elapsed()
            );
        }

        let tree = recovery.finalize();
        tracing::info!(
            "Recovery finished in {:?}; verifying consistency...",
            recovery_started_at.elapsed()
        );
        let started_at = Instant::now();
        tree.verify_consistency(recovered_version).unwrap();
        tracing::info!("Verified consistency in {:?}", started_at.elapsed());
    }
}

fn main() {
    Cli::parse().run();
}

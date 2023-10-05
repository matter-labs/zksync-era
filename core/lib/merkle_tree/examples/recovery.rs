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
use zksync_storage::RocksDB;
use zksync_types::U256;

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
        let key_step =
            Key::MAX / (Key::from(self.update_count) * Key::from(self.writes_per_update));
        let mut last_key = Key::zero();
        let mut last_leaf_index = 0;
        let mut recovery = MerkleTreeRecovery::with_hasher(db, recovered_version, hasher);
        let recovery_started_at = Instant::now();
        for updated_idx in 0..self.update_count {
            let started_at = Instant::now();
            let recovery_entries = (0..self.writes_per_update)
                .map(|_| {
                    last_key += key_step / 2 + gen_key(&mut rng, key_step / 2);
                    // ^ Increases the key by a random increment in [key_step / 2, key_step].
                    last_leaf_index += 1;
                    RecoveryEntry {
                        key: last_key,
                        value: ValueHash::zero(),
                        leaf_index: last_leaf_index,
                        version: rng.gen_range(0..=recovered_version),
                    }
                })
                .collect();
            recovery.extend(recovery_entries);
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

/// Generates a random `Key` in `0..=max` range.
fn gen_key(rng: &mut impl Rng, max: Key) -> Key {
    let output = max & U256([rng.gen(), rng.gen(), rng.gen(), rng.gen()]);
    assert!(output <= max);
    output
}

fn main() {
    Cli::parse().run();
}

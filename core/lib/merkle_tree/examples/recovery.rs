//! Tree recovery load test.

use std::time::Instant;

use anyhow::Context as _;
use clap::Parser;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    recovery::MerkleTreeRecovery, HashTree, Key, MerkleTree, PatchSet, PruneDatabase,
    RocksDBWrapper, TreeEntry, ValueHash,
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
    /// Parallelize DB persistence with processing.
    #[arg(long = "parallelize", conflicts_with = "in_memory")]
    parallelize: bool,
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

    fn run(self) -> anyhow::Result<()> {
        Self::init_logging();
        tracing::info!("Launched with options: {self:?}");

        let hasher: &dyn HashTree = if self.no_hashing { &() } else { &Blake2Hasher };
        let recovered_version = 123;

        if self.in_memory {
            let recovery =
                MerkleTreeRecovery::with_hasher(PatchSet::default(), recovered_version, hasher)?;
            self.recover_tree(recovery, recovered_version)
        } else {
            let dir = TempDir::new().context("failed creating temp dir for RocksDB")?;
            tracing::info!(
                "Created temp dir for RocksDB: {}",
                dir.path().to_string_lossy()
            );
            let db_options = RocksDBOptions {
                block_cache_capacity: self.block_cache,
                ..RocksDBOptions::default()
            };
            let db =
                RocksDB::with_options(dir.path(), db_options).context("failed creating RocksDB")?;
            let db = RocksDBWrapper::from(db);
            let mut recovery = MerkleTreeRecovery::with_hasher(db, recovered_version, hasher)?;
            if self.parallelize {
                recovery.parallelize_persistence(4)?;
            }
            self.recover_tree(recovery, recovered_version)
        }
    }

    fn recover_tree<DB: PruneDatabase>(
        self,
        mut recovery: MerkleTreeRecovery<DB, &dyn HashTree>,
        recovered_version: u64,
    ) -> anyhow::Result<()> {
        let mut rng = StdRng::seed_from_u64(self.rng_seed);

        let key_step =
            Key::MAX / (Key::from(self.update_count) * Key::from(self.writes_per_update));
        assert!(key_step > Key::from(u64::MAX));
        // ^ Total number of generated keys is <2^128.

        let mut last_key = Key::zero();
        let mut last_leaf_index = 0;
        let recovery_started_at = Instant::now();
        for updated_idx in 0..self.update_count {
            let started_at = Instant::now();
            let recovery_entries = (0..self.writes_per_update)
                .map(|_| {
                    last_leaf_index += 1;
                    if self.random {
                        TreeEntry {
                            key: Key::from(rng.gen::<[u8; 32]>()),
                            value: ValueHash::zero(),
                            leaf_index: last_leaf_index,
                        }
                    } else {
                        last_key += key_step - Key::from(rng.gen::<u64>());
                        // ^ Increases the key by a random increment close to `key` step with some randomness.
                        TreeEntry {
                            key: last_key,
                            value: ValueHash::zero(),
                            leaf_index: last_leaf_index,
                        }
                    }
                })
                .collect();
            if self.random {
                recovery
                    .extend_random(recovery_entries)
                    .context("failed extending tree during recovery")?;
            } else {
                recovery
                    .extend_linear(recovery_entries)
                    .context("failed extending tree during recovery")?;
            }
            tracing::info!(
                "Updated tree with recovery chunk #{updated_idx} in {:?}",
                started_at.elapsed()
            );
        }

        let db = recovery
            .finalize()
            .context("failed finalizing tree recovery")?;
        let tree = MerkleTree::new(db).context("tree has invalid metadata after recovery")?;
        tracing::info!(
            "Recovery finished in {:?}; verifying consistency...",
            recovery_started_at.elapsed()
        );
        let started_at = Instant::now();
        tree.verify_consistency(recovered_version, true)
            .context("tree is inconsistent")?;
        tracing::info!("Verified consistency in {:?}", started_at.elapsed());
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    Cli::parse().run()
}

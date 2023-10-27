//! Load-testing for the Merkle tree implementation.
//!
//! Should be compiled with the release profile, otherwise hashing and other ops would be
//! prohibitively slow.

use clap::Parser;
use rand::{rngs::StdRng, seq::IteratorRandom, SeedableRng};
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;

use std::{
    thread,
    time::{Duration, Instant},
};

use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    Database, HashTree, MerkleTree, MerkleTreePruner, PatchSet, RocksDBWrapper, TreeInstruction,
};
use zksync_storage::{RocksDB, RocksDBOptions};
use zksync_types::{AccountTreeId, Address, StorageKey, H256, U256};

mod batch;

use crate::batch::WithBatching;

/// CLI for load-testing for the Merkle tree implementation.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Number of commits to perform.
    #[arg(name = "commits")]
    commit_count: u64,
    /// Number of inserts / updates per commit.
    #[arg(name = "ops")]
    writes_per_commit: usize,
    /// Generate Merkle proofs for each operation.
    #[arg(name = "proofs", long)]
    proofs: bool,
    /// Additional number of reads of previously written keys per commit.
    #[arg(name = "reads", long, default_value = "0", requires = "proofs")]
    reads_per_commit: usize,
    /// Additional number of updates of previously written keys per commit.
    #[arg(name = "updates", long, default_value = "0")]
    updates_per_commit: usize,
    /// Use a no-op hashing function.
    #[arg(name = "no-hash", long)]
    no_hashing: bool,
    /// Perform testing on in-memory DB rather than RocksDB (i.e., with focus on hashing logic).
    #[arg(long = "in-memory", short = 'M')]
    in_memory: bool,
    /// Size of batches used.
    #[arg(long = "batch")]
    batch_size: Option<usize>,
    /// Block cache capacity for RocksDB in bytes.
    #[arg(long = "block-cache", conflicts_with = "in_memory")]
    block_cache: Option<usize>,
    /// Chunk size for RocksDB multi-get operations.
    #[arg(long = "chunk-size", conflicts_with = "in_memory")]
    chunk_size: Option<usize>,
    /// Seed to use in the RNG for reproducibility.
    #[arg(long = "rng-seed", default_value = "0")]
    rng_seed: u64,
    /// Enables tree pruning.
    #[arg(long = "prune", conflicts_with = "in_memory")]
    prune: bool,
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
        let mut pruner_handles = None;
        let mut db: &mut dyn Database = if self.in_memory {
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

            if let Some(chunk_size) = self.chunk_size {
                rocksdb.set_multi_get_chunk_size(chunk_size);
            }

            if self.prune {
                let (mut pruner, pruner_handle) = MerkleTreePruner::new(rocksdb.clone(), 0);
                pruner.set_poll_interval(Duration::from_secs(10));
                let pruner_thread = thread::spawn(|| pruner.run());
                pruner_handles = Some((pruner_handle, pruner_thread));
            }
            _temp_dir = Some(dir);
            &mut rocksdb
        };

        let mut batching_db;
        if let Some(batch_size) = self.batch_size {
            batching_db = WithBatching::new(db, batch_size);
            db = &mut batching_db;
        }

        let hasher: &dyn HashTree = if self.no_hashing { &() } else { &Blake2Hasher };
        let mut rng = StdRng::seed_from_u64(self.rng_seed);

        let mut tree = MerkleTree::with_hasher(db, hasher);
        let mut next_key_idx = 0_u64;
        let mut next_value_idx = 0_u64;
        for version in 0..self.commit_count {
            let new_keys: Vec<_> = Self::generate_keys(next_key_idx..)
                .take(self.writes_per_commit)
                .collect();
            let read_indices = (0..=next_key_idx).choose_multiple(&mut rng, self.reads_per_commit);
            let updated_indices =
                (0..=next_key_idx).choose_multiple(&mut rng, self.updates_per_commit);
            next_key_idx += new_keys.len() as u64;

            next_value_idx += (new_keys.len() + updated_indices.len()) as u64;
            let values = (next_value_idx..).map(H256::from_low_u64_be);
            let updated_keys = Self::generate_keys(updated_indices.into_iter());
            let kvs = new_keys.into_iter().chain(updated_keys).zip(values);

            tracing::info!("Processing block #{version}");
            let start = Instant::now();
            let root_hash = if self.proofs {
                let reads = Self::generate_keys(read_indices.into_iter())
                    .map(|key| (key, TreeInstruction::Read));
                let instructions = kvs
                    .map(|(key, hash)| (key, TreeInstruction::Write(hash)))
                    .chain(reads)
                    .collect();
                let output = tree.extend_with_proofs(instructions);
                output.root_hash().unwrap()
            } else {
                let output = tree.extend(kvs.collect());
                output.root_hash
            };
            let elapsed = start.elapsed();
            tracing::info!("Processed block #{version} in {elapsed:?}, root hash = {root_hash:?}");
        }

        tracing::info!("Verifying tree consistency...");
        let start = Instant::now();
        tree.verify_consistency(self.commit_count - 1)
            .expect("tree consistency check failed");
        let elapsed = start.elapsed();
        tracing::info!("Verified tree consistency in {elapsed:?}");

        if let Some((pruner_handle, pruner_thread)) = pruner_handles {
            pruner_handle.abort();
            pruner_thread.join().unwrap();
        }
    }

    fn generate_keys(key_indexes: impl Iterator<Item = u64>) -> impl Iterator<Item = U256> {
        let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
        key_indexes.map(move |idx| {
            let key = H256::from_low_u64_be(idx);
            let key = StorageKey::new(AccountTreeId::new(address), key);
            key.hashed_key_u256()
        })
    }
}

fn main() {
    Cli::parse().run();
}

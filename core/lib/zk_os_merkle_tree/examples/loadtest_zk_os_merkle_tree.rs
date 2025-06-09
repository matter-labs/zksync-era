// //! Load test for the Merkle tree.

// use std::{hint::black_box, ops, time::Instant};

// use anyhow::Context;
// use clap::Parser;
// use rand::{
//     prelude::{IteratorRandom, StdRng},
//     SeedableRng,
// };
// use tempfile::TempDir;
// use tracing_subscriber::EnvFilter;
// use zk_os_merkle_tree::{
//     unstable, Database, DefaultTreeParams, DeserializeError, HashTree, MerkleTree,
//     MerkleTreeColumnFamily, PatchSet, Patched, RocksDBWrapper, TreeEntry, TreeParams,
// };
// use zksync_basic_types::H256;
// use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};
// use zksync_storage::{db::NamedColumnFamily, RocksDB, RocksDBOptions};

// #[derive(Debug)]
// struct WithDynHasher;

// impl TreeParams for WithDynHasher {
//     type Hasher = &'static dyn HashTree;
//     const TREE_DEPTH: u8 = <DefaultTreeParams>::TREE_DEPTH;
//     const INTERNAL_NODE_DEPTH: u8 = <DefaultTreeParams>::INTERNAL_NODE_DEPTH;
// }

// pub struct WithBatching<'a> {
//     inner: Patched<&'a mut dyn Database>,
//     batch_size: usize,
//     in_memory_batch_size: usize,
// }

// impl<'a> WithBatching<'a> {
//     pub fn new(db: &'a mut dyn Database, batch_size: usize) -> Self {
//         assert!(batch_size > 0, "Batch size must be positive");
//         Self {
//             inner: Patched::new(db),
//             batch_size,
//             in_memory_batch_size: 0,
//         }
//     }
// }

// impl Database for WithBatching<'_> {
//     fn indices(
//         &self,
//         version: u64,
//         keys: &[H256],
//     ) -> Result<Vec<unstable::KeyLookup>, DeserializeError> {
//         self.inner.indices(version, keys)
//     }

//     fn try_manifest(&self) -> Result<Option<unstable::Manifest>, DeserializeError> {
//         self.inner.try_manifest()
//     }

//     fn try_root(&self, version: u64) -> Result<Option<unstable::Root>, DeserializeError> {
//         self.inner.try_root(version)
//     }

//     fn try_nodes(
//         &self,
//         keys: &[unstable::NodeKey],
//     ) -> Result<Vec<unstable::Node>, DeserializeError> {
//         self.inner.try_nodes(keys)
//     }

//     fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
//         self.inner.apply_patch(patch)?;

//         self.in_memory_batch_size += 1;
//         if self.in_memory_batch_size >= self.batch_size {
//             tracing::info!("Flushing changes to underlying DB");
//             self.inner.flush()?;
//             self.in_memory_batch_size = 0;
//         }
//         Ok(())
//     }

//     fn truncate(
//         &mut self,
//         manifest: unstable::Manifest,
//         truncated_versions: ops::RangeTo<u64>,
//     ) -> anyhow::Result<()> {
//         self.inner.flush()?;
//         self.inner.truncate(manifest, truncated_versions)
//     }
// }

// /// CLI for load-testing for the Merkle tree implementation.
// #[derive(Debug, Parser)]
// #[command(author, version, about, long_about = None)]
// struct Cli {
//     /// Number of batches to insert into the tree.
//     #[arg(name = "batches")]
//     batch_count: u64,
//     /// Number of inserts per commit.
//     #[arg(name = "ops")]
//     writes_per_batch: usize,
//     /// Additional number of updates of previously written keys per commit.
//     #[arg(name = "updates", long, default_value = "0")]
//     updates_per_batch: usize,
//     /// Generate Merkle proofs for each operation.
//     #[arg(name = "proofs", long)]
//     proofs: bool,
//     /// Additional number of reads of previously written keys per commit.
//     #[arg(name = "reads", long, default_value = "0", requires = "proofs")]
//     reads_per_batch: usize,
//     /// Interval between flushes to the underlying DB.
//     #[arg(long = "flush-interval")]
//     flush_interval: Option<usize>,
//     /// Use a no-op hashing function.
//     #[arg(name = "no-hash", long)]
//     no_hashing: bool,
//     /// Perform testing on in-memory DB rather than RocksDB (i.e., with focus on hashing logic).
//     #[arg(long = "in-memory", short = 'M')]
//     in_memory: bool,
//     /// Block cache capacity for RocksDB in bytes.
//     #[arg(long = "block-cache", conflicts_with = "in_memory")]
//     block_cache: Option<usize>,
//     /// If specified, RocksDB indices and Bloom filters will be managed by the block cache rather than
//     /// being loaded entirely into RAM.
//     #[arg(long = "cache-indices", conflicts_with = "in_memory")]
//     cache_indices: bool,
//     /// Chunk size for RocksDB multi-get operations.
//     #[arg(long = "chunk-size", conflicts_with = "in_memory")]
//     chunk_size: Option<usize>,
//     /// Seed to use in the RNG for reproducibility.
//     #[arg(long = "rng-seed", default_value = "0")]
//     rng_seed: u64,
// }

// impl Cli {
//     fn init_logging() {
//         tracing_subscriber::fmt()
//             .pretty()
//             .with_env_filter(EnvFilter::from_default_env())
//             .init();
//     }

//     fn run(self) -> anyhow::Result<()> {
//         Self::init_logging();
//         tracing::info!("Launched with options: {self:?}");

//         let mut mock_db;
//         let mut rocksdb = None;
//         let mut _temp_dir = None;
//         let mut db: &mut dyn Database = if self.in_memory {
//             mock_db = PatchSet::default();
//             &mut mock_db
//         } else {
//             let dir = TempDir::new().context("failed creating temp dir for RocksDB")?;
//             tracing::info!(
//                 "Created temp dir for RocksDB: {}",
//                 dir.path().to_string_lossy()
//             );
//             let db_options = RocksDBOptions {
//                 block_cache_capacity: self.block_cache,
//                 include_indices_and_filters_in_block_cache: self.cache_indices,
//                 ..RocksDBOptions::default()
//             };
//             let db =
//                 RocksDB::with_options(dir.path(), db_options).context("failed creating RocksDB")?;
//             let mut db = RocksDBWrapper::from(db);

//             if let Some(chunk_size) = self.chunk_size {
//                 db.set_multi_get_chunk_size(chunk_size);
//             }

//             _temp_dir = Some(dir);
//             rocksdb = Some(db);
//             rocksdb.as_mut().unwrap()
//         };

//         let mut batching_db;
//         if let Some(flush_interval) = self.flush_interval {
//             batching_db = WithBatching::new(db, flush_interval);
//             db = &mut batching_db;
//         }

//         let hasher: &dyn HashTree = if self.no_hashing { &() } else { &Blake2Hasher };
//         let mut rng = StdRng::seed_from_u64(self.rng_seed);

//         let mut tree = MerkleTree::<_, WithDynHasher>::with_hasher(db, hasher)
//             .context("cannot create tree")?;
//         let mut next_key_idx = 0_u64;
//         let mut next_value_idx = 0_u64;
//         for version in 0..self.batch_count {
//             let new_keys: Vec<_> = Self::generate_keys(next_key_idx..)
//                 .take(self.writes_per_batch)
//                 .collect();
//             let updated_indices =
//                 (0..next_key_idx).choose_multiple(&mut rng, self.updates_per_batch);
//             let read_indices = (0..next_key_idx).choose_multiple(&mut rng, self.reads_per_batch);
//             next_key_idx += new_keys.len() as u64;

//             next_value_idx += (new_keys.len() + updated_indices.len()) as u64;
//             let updated_keys = Self::generate_keys(updated_indices.into_iter());
//             let kvs = new_keys
//                 .into_iter()
//                 .chain(updated_keys)
//                 .zip(next_value_idx..);
//             let kvs = kvs.map(|(key, idx)| TreeEntry {
//                 key,
//                 value: H256::from_low_u64_be(idx),
//             });
//             let kvs = kvs.collect::<Vec<_>>();

//             tracing::info!("Processing block #{version}");
//             let start = Instant::now();
//             let output = if self.proofs {
//                 let read_keys: Vec<_> = Self::generate_keys(read_indices.into_iter()).collect();
//                 let (output, proof) = tree
//                     .extend_with_proof(&kvs, &read_keys)
//                     .context("failed extending tree")?;
//                 black_box(proof); // Ensure that proof creation isn't optimized away
//                 output
//             } else {
//                 tree.extend(&kvs).context("failed extending tree")?
//             };
//             let root_hash = output.root_hash;

//             let elapsed = start.elapsed();
//             tracing::info!("Processed block #{version} in {elapsed:?}, root hash = {root_hash:?}");
//         }

//         tracing::info!("Verifying tree consistency...");
//         let start = Instant::now();
//         tree.verify_consistency(self.batch_count - 1)
//             .context("tree consistency check failed")?;
//         let elapsed = start.elapsed();
//         tracing::info!("Verified tree consistency in {elapsed:?}");

//         if let Some(db) = rocksdb {
//             let db = db.into_inner();
//             for cf in MerkleTreeColumnFamily::ALL {
//                 tracing::info!(?cf, size = ?db.size_stats(*cf), "estimated DB size");
//             }
//         }

//         Ok(())
//     }

//     fn generate_keys(key_indexes: impl Iterator<Item = u64>) -> impl Iterator<Item = H256> {
//         key_indexes.map(move |idx| {
//             let key = H256::from_low_u64_be(idx);
//             Blake2Hasher.hash_bytes(key.as_bytes())
//         })
//     }
// }

// fn main() -> anyhow::Result<()> {
//     Cli::parse().run()
// }

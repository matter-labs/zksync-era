//! Tying the Merkle tree implementation to the problem domain.

use rayon::{ThreadPool, ThreadPoolBuilder};

use std::{borrow::Borrow, num::NonZeroU32};

use crate::{
    types::TREE_DEPTH, Database, HashTree, Key, MerkleTree, Patched, RocksDBWrapper, Root,
    TreeInstruction, TreeLogEntry, ValueHash,
};
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_storage::RocksDB;
use zksync_types::{
    proofs::{PrepareBasicCircuitsJob, StorageLogMetadata},
    writes::{InitialStorageWrite, RepeatedStorageWrite},
    L1BatchNumber, StorageLogKind, WitnessStorageLog,
};

/// Metadata for the current tree state.
#[derive(Debug, Clone)]
pub struct TreeMetadata {
    /// Current root hash of the tree.
    pub root_hash: ValueHash,
    /// 1-based index of the next leaf to be inserted in the tree.
    pub rollup_last_leaf_index: u64,
    /// Initial writes performed in the processed block.
    pub initial_writes: Vec<InitialStorageWrite>,
    /// Repeated writes performed in the processed block.
    pub repeated_writes: Vec<RepeatedStorageWrite>,
    /// Witness information.
    pub witness: Option<PrepareBasicCircuitsJob>,
}

#[derive(Debug, PartialEq, Eq)]
enum TreeMode {
    Lightweight,
    Full,
}

/// Domain-specific wrapper of the Merkle tree.
///
/// This wrapper will accumulate changes introduced by [`Self::process_block()`],
/// [`Self::process_blocks()`] and [`Self::revert_logs()`] in RAM without saving them
/// to RocksDB. The accumulated changes can be saved to RocksDB via [`Self::save()`]
/// or discarded via [`Self::reset()`].
#[derive(Debug)]
pub struct ZkSyncTree {
    database: Patched<RocksDBWrapper>,
    thread_pool: Option<ThreadPool>,
    mode: TreeMode,
}

impl ZkSyncTree {
    // A reasonable chunk size for RocksDB multi-get operations. Obtained as a result
    // of local benchmarking.
    const MULTI_GET_CHUNK_SIZE: usize = 500;

    fn create_thread_pool(thread_count: usize) -> ThreadPool {
        ThreadPoolBuilder::new()
            .thread_name(|idx| format!("new-merkle-tree-{idx}"))
            .num_threads(thread_count)
            .build()
            .expect("failed initializing `rayon` thread pool")
    }

    /// Creates a tree with the full processing mode.
    pub fn new(db: RocksDB) -> Self {
        Self::new_with_mode(db, TreeMode::Full)
    }

    /// Creates a tree with the lightweight processing mode.
    pub fn new_lightweight(db: RocksDB) -> Self {
        Self::new_with_mode(db, TreeMode::Lightweight)
    }

    fn new_with_mode(db: RocksDB, mode: TreeMode) -> Self {
        let mut wrapper = RocksDBWrapper::from(db);
        wrapper.set_multi_get_chunk_size(Self::MULTI_GET_CHUNK_SIZE);
        Self {
            database: Patched::new(wrapper),
            thread_pool: None,
            mode,
        }
    }

    /// Signals that the tree should use a dedicated `rayon` thread pool for parallel operations
    /// (for now, hash computations).
    ///
    /// If `thread_count` is 0, the default number of threads will be used; see `rayon` docs
    /// for details.
    pub fn use_dedicated_thread_pool(&mut self, thread_count: usize) {
        self.thread_pool = Some(Self::create_thread_pool(thread_count));
    }

    /// Returns the current root hash of this tree.
    pub fn root_hash(&self) -> ValueHash {
        let tree = MerkleTree::new(&self.database);
        tree.latest_root_hash()
    }

    /// Checks whether this tree is empty.
    pub fn is_empty(&self) -> bool {
        let tree = MerkleTree::new(&self.database);
        let Some(version) = tree.latest_version() else {
            return true;
        };
        tree.root(version)
            .map_or(true, |root| matches!(root, Root::Empty))
    }

    /// Returns the current block number.
    pub fn block_number(&self) -> u32 {
        let tree = MerkleTree::new(&self.database);
        tree.latest_version().map_or(0, |version| {
            u32::try_from(version + 1).expect("integer overflow for block number")
        })
    }

    /// Verifies tree consistency. `block_number`, if provided, specifies the version of the tree
    /// to be checked, expressed as the number of blocks applied to the tree. By default,
    /// the latest tree version is checked.
    ///
    /// # Panics
    ///
    /// Panics if an inconsistency is detected.
    pub fn verify_consistency(&self, block_number: NonZeroU32) {
        let tree = MerkleTree::new(&self.database);
        let version = u64::from(block_number.get() - 1);
        tree.verify_consistency(version).unwrap_or_else(|err| {
            panic!("Tree at version {version} is inconsistent: {err}");
        });
    }

    /// Processes an iterator of block logs comprising a single block.
    pub fn process_block(&mut self, storage_logs: &[WitnessStorageLog]) -> TreeMetadata {
        match self.mode {
            TreeMode::Full => self.process_block_full(storage_logs),
            TreeMode::Lightweight => self.process_block_lightweight(storage_logs),
        }
    }

    fn process_block_full(&mut self, storage_logs: &[WitnessStorageLog]) -> TreeMetadata {
        let instructions = Self::transform_logs(storage_logs);
        let tree = MerkleTree::new(&self.database);
        let starting_leaf_count = tree.latest_root().leaf_count();

        let (output, patch) = if let Some(thread_pool) = &self.thread_pool {
            thread_pool.install(|| tree.extend_with_proofs(instructions.clone()))
        } else {
            tree.extend_with_proofs(instructions.clone())
        };
        self.database.apply_patch(patch);

        let mut witness = PrepareBasicCircuitsJob::new(starting_leaf_count + 1);
        witness.reserve(output.logs.len());
        for (log, (key, instruction)) in output.logs.iter().zip(&instructions) {
            let empty_levels_end = TREE_DEPTH - log.merkle_path.len();
            let empty_subtree_hashes =
                (0..empty_levels_end).map(|i| Blake2Hasher.empty_subtree_hash(i));
            let merkle_paths = log.merkle_path.iter().copied();
            let merkle_paths = empty_subtree_hashes
                .chain(merkle_paths)
                .map(|hash| hash.0)
                .collect();

            let log = StorageLogMetadata {
                root_hash: log.root_hash.0,
                is_write: !log.base.is_read(),
                first_write: matches!(log.base, TreeLogEntry::Inserted { .. }),
                merkle_paths,
                leaf_hashed_key: *key,
                leaf_enumeration_index: match log.base {
                    TreeLogEntry::Updated { leaf_index, .. }
                    | TreeLogEntry::Inserted { leaf_index }
                    | TreeLogEntry::Read { leaf_index, .. } => leaf_index,
                    TreeLogEntry::ReadMissingKey => 0,
                },
                value_written: match instruction {
                    TreeInstruction::Write(value) => value.0,
                    TreeInstruction::Read => [0_u8; 32],
                },
                value_read: match log.base {
                    TreeLogEntry::Updated { previous_value, .. } => previous_value.0,
                    TreeLogEntry::Read { value, .. } => value.0,
                    TreeLogEntry::Inserted { .. } | TreeLogEntry::ReadMissingKey => [0_u8; 32],
                },
            };
            witness.push_merkle_path(log);
        }

        let root_hash = output.root_hash().unwrap();
        let logs = output
            .logs
            .into_iter()
            .filter_map(|log| (!log.base.is_read()).then_some(log.base));
        let kvs = instructions.into_iter().filter_map(|(key, instruction)| {
            let TreeInstruction::Write(value) = instruction else {
                return None;
            };
            Some((key, value))
        });
        let (initial_writes, repeated_writes) = Self::extract_writes(logs, kvs);

        TreeMetadata {
            root_hash,
            rollup_last_leaf_index: output.leaf_count + 1,
            initial_writes,
            repeated_writes,
            witness: Some(witness),
        }
    }

    fn transform_logs(storage_logs: &[WitnessStorageLog]) -> Vec<(Key, TreeInstruction)> {
        let instructions = storage_logs.iter().map(|log| {
            let log = &log.storage_log;
            let key = log.key.hashed_key_u256();
            let instruction = match log.kind {
                StorageLogKind::Write => TreeInstruction::Write(log.value),
                StorageLogKind::Read => TreeInstruction::Read,
            };
            (key, instruction)
        });
        instructions.collect()
    }

    fn extract_writes(
        logs: impl Iterator<Item = TreeLogEntry>,
        kvs: impl Iterator<Item = (Key, ValueHash)>,
    ) -> (Vec<InitialStorageWrite>, Vec<RepeatedStorageWrite>) {
        let mut initial_writes = vec![];
        let mut repeated_writes = vec![];
        for (log_entry, (key, value)) in logs.zip(kvs) {
            match log_entry {
                TreeLogEntry::Inserted { .. } => {
                    initial_writes.push(InitialStorageWrite { key, value });
                }
                TreeLogEntry::Updated { leaf_index, .. } => {
                    repeated_writes.push(RepeatedStorageWrite {
                        index: leaf_index,
                        value,
                    });
                }
                TreeLogEntry::Read { .. } | TreeLogEntry::ReadMissingKey => {}
            }
        }
        (initial_writes, repeated_writes)
    }

    fn process_block_lightweight(&mut self, storage_logs: &[WitnessStorageLog]) -> TreeMetadata {
        let kvs = Self::filter_write_logs(storage_logs);
        let tree = MerkleTree::new(&self.database);
        let (output, patch) = if let Some(thread_pool) = &self.thread_pool {
            thread_pool.install(|| tree.extend(kvs.clone()))
        } else {
            tree.extend(kvs.clone())
        };
        self.database.apply_patch(patch);
        let (initial_writes, repeated_writes) =
            Self::extract_writes(output.logs.into_iter(), kvs.into_iter());

        TreeMetadata {
            root_hash: output.root_hash,
            rollup_last_leaf_index: output.leaf_count + 1,
            initial_writes,
            repeated_writes,
            witness: None,
        }
    }

    fn filter_write_logs(storage_logs: &[WitnessStorageLog]) -> Vec<(Key, ValueHash)> {
        let kvs = storage_logs.iter().filter_map(|log| {
            let log = &log.borrow().storage_log;
            match log.kind {
                StorageLogKind::Write => {
                    let key = log.key.hashed_key_u256();
                    Some((key, log.value))
                }
                StorageLogKind::Read => None,
            }
        });
        kvs.collect()
    }

    /// Reverts the tree to a previous state.
    ///
    /// Just like [`Self::process_block()`], this method will overwrite all unsaved changes
    /// in the tree.
    pub fn revert_logs(&mut self, block_number: L1BatchNumber) {
        self.database.reset();

        let block_number = u64::from(block_number.0 + 1);
        let tree = MerkleTree::new(&self.database);
        if let Some(patch) = tree.truncate_versions(block_number) {
            self.database.apply_patch(patch);
        }
    }

    /// Saves the accumulated changes in the tree to RocksDB. This method or [`Self::reset()`]
    /// should be called after each `process_block()` / `process_blocks()` / `revert_logs()`
    /// call; otherwise, the changes produced by the processing method call will be lost
    /// on subsequent calls.
    pub fn save(&mut self) {
        self.database.flush();
    }

    /// Resets the tree to the latest database state.
    pub fn reset(&mut self) {
        self.database.reset();
    }
}

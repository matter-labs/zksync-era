use crate::types::{
    InitialStorageWrite, LeafIndices, LevelIndex, RepeatedStorageWrite, TreeKey, TreeOperation,
    ZkHash,
};
use crate::TreeError;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use zksync_storage::db::MerkleTreeColumnFamily;
use zksync_storage::rocksdb::WriteBatch;
use zksync_storage::util::{deserialize_block_number, serialize_block_number, serialize_tree_leaf};
use zksync_storage::RocksDB;

const BLOCK_NUMBER_KEY: &[u8; 12] = b"block_number";
const LEAF_INDEX_KEY: &[u8; 10] = b"leaf_index";

// Represents pending update that is yet to be flushed in RocksDB.
#[derive(Default)]
struct PendingPatch(WriteBatch);

impl Debug for PendingPatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingPatch {{ ... }}")
    }
}

/// Storage wrapper around RocksDB.
/// Stores hashes of branch nodes in merkle tree and current block number
#[derive(Debug)]
pub struct Storage {
    db: RocksDB,
    pending_patch: PendingPatch,
}

impl Storage {
    pub fn new(db: RocksDB) -> Self {
        Self {
            db,
            pending_patch: PendingPatch(WriteBatch::default()),
        }
    }

    /// Fetches hashes of merkle tree branches from db
    pub fn hashes<'a, I: 'a>(&'a self, keys: I) -> Vec<Option<Vec<u8>>>
    where
        I: IntoIterator<Item = &'a LevelIndex>,
    {
        self.db
            .multi_get(keys.into_iter().map(LevelIndex::bin_key))
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Prepares db update
    pub fn pre_save(&mut self, branches: HashMap<LevelIndex, Vec<u8>>) {
        for (level_index, value) in branches {
            self.pending_patch.0.put(level_index.bin_key(), value);
        }
    }

    /// Saves current state to db
    pub fn save(&mut self, block_number: u32) -> Result<(), TreeError> {
        if self.pending_patch.0.is_empty() {
            return Err(TreeError::EmptyPatch);
        }
        let mut write_batch =
            std::mem::replace(&mut self.pending_patch, PendingPatch(WriteBatch::default())).0;
        write_batch.put(BLOCK_NUMBER_KEY, serialize_block_number(block_number));

        // Sync write is not used here intentionally. It somewhat improves write performance.
        // Overall flow is designed in such way that data is committed to state keeper first
        // and, in case of process crash, tree state is recoverable
        self.db
            .write(write_batch)
            .map_err(TreeError::StorageIoError)
    }

    /// Updates mapping between leaf index and its historical first occurrence and returns it
    ///
    /// note: for simplicity this column family update is done separately from the main one
    /// so column families can become out of sync in the case of intermediate process crash
    /// but after restart state is fully recoverable
    pub fn process_leaf_indices(
        &mut self,
        storage_logs: &[(usize, (TreeKey, TreeOperation))],
    ) -> Result<Vec<LeafIndices>, TreeError> {
        let cf = self
            .db
            .cf_merkle_tree_handle(MerkleTreeColumnFamily::LeafIndices);
        let mut current_index = self
            .db
            .get_cf(cf, LEAF_INDEX_KEY)
            .expect("failed to fetch current leaf index")
            .map(|bytes| deserialize_leaf_index(&bytes))
            .unwrap_or(1);

        let mut write_batch = std::mem::take(&mut self.pending_patch).0;
        let mut new_writes = HashMap::new();

        let result = self
            .db
            .multi_get_cf(
                storage_logs
                    .iter()
                    .map(|(_, (key, _))| (cf, serialize_tree_leaf(*key))),
            )
            .into_iter()
            .zip(storage_logs)
            .group_by(|(_, &(block, _))| block)
            .into_iter()
            .map(|(_block, group)| {
                let mut repeated_writes = Vec::new();
                let mut initial_writes = Vec::new();
                let previous_index = current_index;

                let leaf_indices = group
                    .map(|(raw_data, &(_, (leaf, tree_operation)))| {
                        let leaf_index = match (
                            raw_data.expect("failed to fetch leaf index"),
                            tree_operation,
                        ) {
                            // revert of first occurrence
                            (Some(_), TreeOperation::Delete) => {
                                write_batch.delete_cf(cf, serialize_tree_leaf(leaf));
                                current_index -= 1;
                                0
                            }
                            // leaf doesn't exist
                            (None, TreeOperation::Delete) => 0,
                            // existing leaf
                            (Some(bytes), TreeOperation::Write { value, .. }) => {
                                let index = deserialize_leaf_index(&bytes);
                                repeated_writes.push(RepeatedStorageWrite { index, value });
                                index
                            }
                            (Some(bytes), TreeOperation::Read(_)) => deserialize_leaf_index(&bytes),
                            // first occurrence read (noop)
                            (None, TreeOperation::Read(_)) => *new_writes.get(&leaf).unwrap_or(&0),
                            // first occurrence write
                            (None, TreeOperation::Write { value, .. }) => {
                                // Since there can't be 2 logs for the same slot in one block,
                                // we can safely assume that if we have a new write, it was done in a
                                // previous block and thus the new index is valid.
                                if let Some(&index) = new_writes.get(&leaf) {
                                    repeated_writes.push(RepeatedStorageWrite { index, value });
                                    index
                                } else {
                                    let index = current_index;
                                    write_batch.put_cf(
                                        cf,
                                        serialize_tree_leaf(leaf),
                                        serialize_leaf_index(index),
                                    );
                                    initial_writes.push(InitialStorageWrite { key: leaf, value });
                                    new_writes.insert(leaf, index);
                                    current_index += 1;
                                    index
                                }
                            }
                        };
                        (leaf, leaf_index)
                    })
                    .collect();

                LeafIndices {
                    leaf_indices,
                    previous_index,
                    initial_writes,
                    repeated_writes,
                    last_index: current_index,
                }
            })
            .collect();

        write_batch.put_cf(cf, LEAF_INDEX_KEY, serialize_leaf_index(current_index));
        self.pending_patch = PendingPatch(write_batch);

        Ok(result)
    }

    /// Fetches high-level metadata about merkle tree state
    pub fn fetch_metadata(&self) -> StoredTreeMetadata {
        // Fetch root hash. It is represented by level index (0, 0).
        let root_hash = self.hashes(vec![&(0, 0.into()).into()])[0].clone();

        let block_number = self
            .db
            .get(BLOCK_NUMBER_KEY)
            .expect("failed to fetch tree metadata")
            .map(|bytes| deserialize_block_number(&bytes))
            .unwrap_or(0);
        (root_hash, block_number)
    }
}

/// High level merkle tree metadata
/// Includes root hash and current block number
pub(crate) type StoredTreeMetadata = (Option<ZkHash>, u32);

pub(crate) fn serialize_leaf_index(leaf_index: u64) -> Vec<u8> {
    let mut bytes = vec![0; 8];
    BigEndian::write_u64(&mut bytes, leaf_index);
    bytes
}

fn deserialize_leaf_index(mut bytes: &[u8]) -> u64 {
    bytes
        .read_u64::<BigEndian>()
        .expect("failed to deserialize leaf index")
}

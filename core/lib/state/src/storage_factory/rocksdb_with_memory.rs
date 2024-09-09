use std::collections::HashMap;

use zksync_types::{StorageKey, StorageValue, H256};
use zksync_vm_interface::storage::ReadStorage;

use crate::RocksdbStorage;

/// DB difference introduced by one batch.
#[derive(Debug, Clone)]
pub struct BatchDiff {
    /// Storage slots touched by this batch along with new values there.
    pub state_diff: HashMap<H256, H256>,
    /// Initial write indices introduced by this batch.
    pub enum_index_diff: HashMap<H256, u64>,
    /// Factory dependencies introduced by this batch.
    pub factory_dep_diff: HashMap<H256, Vec<u8>>,
}

/// A RocksDB cache instance with in-memory DB diffs that gives access to DB state at batches `N` to
/// `N + K`, where `K` is the number of diffs.
#[derive(Debug)]
pub struct RocksdbWithMemory {
    /// RocksDB cache instance caught up to batch `N`.
    pub rocksdb: RocksdbStorage,
    /// Diffs for batches `N + 1` to `N + K`.
    pub batch_diffs: Vec<BatchDiff>,
}

impl ReadStorage for RocksdbWithMemory {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let hashed_key = key.hashed_key();
        match self
            .batch_diffs
            .iter()
            .rev()
            .find_map(|b| b.state_diff.get(&hashed_key))
        {
            None => self.rocksdb.read_value(key),
            Some(value) => *value,
        }
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.enum_index_diff.get(&key.hashed_key()))
        {
            None => self.rocksdb.is_write_initial(key),
            Some(_) => false,
        }
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.factory_dep_diff.get(&hash))
        {
            None => self.rocksdb.load_factory_dep(hash),
            Some(value) => Some(value.clone()),
        }
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.enum_index_diff.get(&key.hashed_key()))
        {
            None => self.rocksdb.get_enumeration_index(key),
            Some(value) => Some(*value),
        }
    }
}

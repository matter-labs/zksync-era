use std::collections::{HashMap, VecDeque};

use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};
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

/// Data structure that keeps a continuous list of batch diffs.
#[derive(Debug)]
pub struct BatchDiffs {
    diffs: VecDeque<BatchDiff>,
    first_diff_l1_batch_number: Option<L1BatchNumber>,
}

impl BatchDiffs {
    pub fn new() -> Self {
        Self {
            diffs: Default::default(),
            first_diff_l1_batch_number: None
        }
    }

    /// Trims diffs that correspond to batches with number less than `trim_up_to`. Does nothing if there are no.
    pub fn trim_start(&mut self, trim_up_to: L1BatchNumber) {
        let Some(first_diff_batch_number) = self.first_diff_l1_batch_number
        else {
            return;
        };

        if first_diff_batch_number < trim_up_to {
            let split_at = (trim_up_to.0 - first_diff_batch_number.0) as usize;
            self.diffs = self.diffs.split_off(split_at);
            if self.diffs.is_empty() {
                self.first_diff_l1_batch_number = None;
            } else {
                self.first_diff_l1_batch_number = Some(trim_up_to);
            }
        }
    }

    /// Pushes the diff.
    pub fn push(&mut self, l1_batch_number: L1BatchNumber, diff: BatchDiff) {
        if self.first_diff_l1_batch_number.is_none() {
            self.first_diff_l1_batch_number = Some(l1_batch_number);
        }
        self.diffs.push_back(diff);
    }

    /// Returns diffs for `from_l1_batch..=to_l1_batch`.
    /// Panics if there is no diff for some element in the range.
    pub fn range(&self, from_l1_batch: L1BatchNumber, to_l1_batch: L1BatchNumber) -> Vec<BatchDiff> {
        let first_diff_number = self.first_diff_l1_batch_number.expect("empty batch_diffs");
        assert!(from_l1_batch >= first_diff_number);
        assert!(to_l1_batch < first_diff_number + self.diffs.len() as u32);

        let to_skip = from_l1_batch.0 - first_diff_number.0;
        let to_take = to_l1_batch.0 - from_l1_batch.0 + 1;

        self.diffs
            .iter()
            .skip(to_skip as usize)
            .take(to_take as usize)
            .cloned()
            .collect()
    }
}

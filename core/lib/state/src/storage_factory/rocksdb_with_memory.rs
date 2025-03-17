use std::{
    collections::{HashMap, VecDeque},
    ops::RangeInclusive,
};

use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};
use zksync_vm_interface::storage::ReadStorage;

use crate::RocksdbStorage;

/// DB difference introduced by one batch.
#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Default)]
pub struct BatchDiffs {
    diffs: VecDeque<BatchDiff>,
    first_diff_l1_batch_number: Option<L1BatchNumber>,
}

impl BatchDiffs {
    /// Creates empty `BatchDiffs`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Trims diffs that correspond to batches with number less than `trim_up_to`. Does nothing if there are no.
    pub fn trim_start(&mut self, trim_up_to: L1BatchNumber) {
        let Some(first_diff_batch_number) = self.first_diff_l1_batch_number else {
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
    ///
    /// # Panics
    ///
    /// Panics if the batch number of the pushed diff is not sequential.
    pub fn push(&mut self, l1_batch_number: L1BatchNumber, diff: BatchDiff) {
        if let Some(first_diff_l1_batch_number) = self.first_diff_l1_batch_number {
            let next_expected_batch_number =
                first_diff_l1_batch_number + u32::try_from(self.diffs.len()).unwrap();
            assert_eq!(l1_batch_number, next_expected_batch_number);
        } else {
            self.first_diff_l1_batch_number = Some(l1_batch_number);
        }
        self.diffs.push_back(diff);
    }

    /// Returns diffs for `from_l1_batch..=to_l1_batch`.
    ///
    /// # Panics
    ///
    /// Panics if there is no diff for some element in the range.
    pub(crate) fn range(&self, batch_range: RangeInclusive<L1BatchNumber>) -> Vec<BatchDiff> {
        let from_l1_batch = *batch_range.start();
        let to_l1_batch = *batch_range.end();

        let first_diff_number = self.first_diff_l1_batch_number.expect("empty batch_diffs");
        assert!(from_l1_batch >= first_diff_number);
        assert!((to_l1_batch.0 as usize) < (first_diff_number.0 as usize) + self.diffs.len());

        let relative_start_index = (from_l1_batch.0 - first_diff_number.0) as usize;
        let relative_end_index = (to_l1_batch.0 - first_diff_number.0) as usize;

        self.diffs
            .range(relative_start_index..=relative_end_index)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_diffs_basics() {
        let mut diffs = BatchDiffs::new();

        diffs.push(L1BatchNumber(1), BatchDiff::default());
        let res = diffs.range(L1BatchNumber(1)..=L1BatchNumber(1));
        assert_eq!(res.len(), 1);

        diffs.push(L1BatchNumber(2), BatchDiff::default());
        diffs.push(L1BatchNumber(3), BatchDiff::default());
        let res = diffs.range(L1BatchNumber(1)..=L1BatchNumber(3));
        assert_eq!(res.len(), 3);

        diffs.trim_start(L1BatchNumber(2));
        let res = diffs.range(L1BatchNumber(2)..=L1BatchNumber(3));
        assert_eq!(res.len(), 2);
    }

    #[test]
    #[should_panic(expected = "assertion failed: from_l1_batch >= first_diff_number")]
    fn batch_diffs_panics() {
        let mut diffs = BatchDiffs::new();

        diffs.push(L1BatchNumber(1), BatchDiff::default());
        diffs.push(L1BatchNumber(2), BatchDiff::default());
        diffs.push(L1BatchNumber(3), BatchDiff::default());

        diffs.trim_start(L1BatchNumber(2));
        diffs.range(L1BatchNumber(1)..=L1BatchNumber(3));
    }
}

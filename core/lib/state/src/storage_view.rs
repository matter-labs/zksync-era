use std::cell::RefCell;
use std::rc::Rc;
use std::{
    collections::HashMap,
    fmt, mem,
    time::{Duration, Instant},
};

use crate::{ReadStorage, WriteStorage};
use zksync_types::{witness_block_state::WitnessBlockState, StorageKey, StorageValue, H256};

/// Metrics for [`StorageView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageViewMetrics {
    /// Estimated byte size of the cache used by the `StorageView`.
    pub cache_size: usize,
    /// Number of read / write ops for which the value was read from the underlying storage.
    pub storage_invocations_missed: usize,
    /// Number of processed read ops.
    pub get_value_storage_invocations: usize,
    /// Number of processed write ops.
    pub set_value_storage_invocations: usize,
    /// Cumulative time spent on reading data from the underlying storage.
    pub time_spent_on_storage_missed: Duration,
    /// Cumulative time spent on all read ops.
    pub time_spent_on_get_value: Duration,
    /// Cumulative time spent on all write ops.
    pub time_spent_on_set_value: Duration,
}

/// `StorageView` is a buffer for `StorageLog`s between storage and transaction execution code.
/// In order to commit transactions logs should be submitted to the underlying storage
/// after a transaction is executed.
///
/// When executing transactions as a part of miniblock / L1 batch creation,
/// a single `StorageView` is used for the entire L1 batch.
/// One `StorageView` must not be used for multiple L1 batches;
/// otherwise, [`Self::is_write_initial()`] will return incorrect values because of the caching.
///
/// When executing transactions in the API sandbox, a dedicated view is used for each transaction;
/// the only shared part is the read storage keys cache.
#[derive(Debug)]
pub struct StorageView<S> {
    storage_handle: S,
    // Used for caching and to get the list/count of modified keys
    modified_storage_keys: HashMap<StorageKey, StorageValue>,
    // Used purely for caching
    read_storage_keys: HashMap<StorageKey, StorageValue>,
    // Cache for `contains_key()` checks. The cache is only valid within one L1 batch execution.
    initial_writes_cache: HashMap<StorageKey, bool>,
    metrics: StorageViewMetrics,
}

impl<S> StorageView<S> {
    /// Returns the block's start state using StorageView's in-memory cache for the run
    pub fn witness_block_state(&self) -> WitnessBlockState {
        WitnessBlockState {
            read_storage_key: self.read_storage_keys.clone(),
            is_write_initial: self.initial_writes_cache.clone(),
        }
    }
}

impl<S> ReadStorage for Box<S>
where
    S: ReadStorage + ?Sized,
{
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        (**self).read_value(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        (**self).is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        (**self).load_factory_dep(hash)
    }

    fn is_bytecode_known(&mut self, bytecode_hash: &H256) -> bool {
        (**self).is_bytecode_known(bytecode_hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        (**self).get_enumeration_index(key)
    }
}

impl<S: ReadStorage + fmt::Debug> StorageView<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage_handle: S) -> Self {
        Self {
            storage_handle,
            modified_storage_keys: HashMap::new(),
            read_storage_keys: HashMap::new(),
            initial_writes_cache: HashMap::new(),
            metrics: StorageViewMetrics::default(),
        }
    }

    fn get_value_no_log(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();

        let cached_value = self
            .modified_storage_keys
            .get(key)
            .or_else(|| self.read_storage_keys.get(key));
        cached_value.copied().unwrap_or_else(|| {
            let value = self.storage_handle.read_value(key);
            self.read_storage_keys.insert(*key, value);
            self.metrics.time_spent_on_storage_missed += started_at.elapsed();
            self.metrics.storage_invocations_missed += 1;
            value
        })
    }

    fn cache_size(&self) -> usize {
        self.modified_storage_keys.len() * mem::size_of::<(StorageKey, StorageValue)>()
            + self.initial_writes_cache.len() * mem::size_of::<(StorageKey, bool)>()
            + self.read_storage_keys.len() * mem::size_of::<(StorageKey, StorageValue)>()
    }

    /// Returns the current metrics.
    pub fn metrics(&self) -> StorageViewMetrics {
        StorageViewMetrics {
            cache_size: self.cache_size(),
            ..self.metrics
        }
    }

    /// Make a Rc RefCell ptr to the storage
    pub fn to_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageView<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();
        self.metrics.get_value_storage_invocations += 1;
        let value = self.get_value_no_log(key);

        tracing::trace!(
            "read value {:?} {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value.0,
            key.address(),
            key.key()
        );

        self.metrics.time_spent_on_get_value += started_at.elapsed();
        value
    }

    /// Only keys contained in the underlying storage will return `false`. If a key was
    /// inserted using [`Self::set_value()`], it will still return `true`.
    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        if let Some(&is_write_initial) = self.initial_writes_cache.get(key) {
            is_write_initial
        } else {
            let is_write_initial = self.storage_handle.is_write_initial(key);
            self.initial_writes_cache.insert(*key, is_write_initial);
            is_write_initial
        }
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.storage_handle.load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.storage_handle.get_enumeration_index(key)
    }
}

impl<S: ReadStorage + fmt::Debug> WriteStorage for StorageView<S> {
    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue {
        let started_at = Instant::now();
        self.metrics.set_value_storage_invocations += 1;
        let original = self.get_value_no_log(&key);

        tracing::trace!(
            "write value {:?} value: {:?} original value: {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value,
            original,
            key.address(),
            key.key()
        );
        self.modified_storage_keys.insert(key, value);
        self.metrics.time_spent_on_set_value += started_at.elapsed();

        original
    }

    fn modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.modified_storage_keys
    }

    fn missed_storage_invocations(&self) -> usize {
        self.metrics.storage_invocations_missed
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::InMemoryStorage;
    use zksync_types::{AccountTreeId, Address, H256};

    #[test]
    fn test_storage_access() {
        let account: AccountTreeId = AccountTreeId::new(Address::from([0xfe; 20]));
        let key = H256::from_low_u64_be(61);
        let value = H256::from_low_u64_be(73);
        let key = StorageKey::new(account, key);

        let mut raw_storage = InMemoryStorage::default();
        let mut storage_view = StorageView::new(&raw_storage);

        let default_value = storage_view.read_value(&key);
        assert_eq!(default_value, H256::zero());

        let prev_value = storage_view.set_value(key, value);
        assert_eq!(prev_value, H256::zero());
        assert_eq!(storage_view.read_value(&key), value);
        assert!(storage_view.is_write_initial(&key)); // key was inserted during the view lifetime

        assert_eq!(storage_view.metrics().storage_invocations_missed, 1);
        // ^ We should only read a value at `key` once, and then used the cached value.

        raw_storage.set_value(key, value);
        let mut storage_view = StorageView::new(&raw_storage);

        assert_eq!(storage_view.read_value(&key), value);
        assert!(!storage_view.is_write_initial(&key)); // `key` is present in `raw_storage`

        let new_value = H256::from_low_u64_be(74);
        storage_view.set_value(key, new_value);
        assert_eq!(storage_view.read_value(&key), new_value);

        let new_key = StorageKey::new(account, H256::from_low_u64_be(62));
        storage_view.set_value(new_key, new_value);
        assert_eq!(storage_view.read_value(&new_key), new_value);
        assert!(storage_view.is_write_initial(&new_key));

        let metrics = storage_view.metrics();
        assert_eq!(metrics.storage_invocations_missed, 2);
        assert_eq!(metrics.get_value_storage_invocations, 3);
        assert_eq!(metrics.set_value_storage_invocations, 2);
    }
}

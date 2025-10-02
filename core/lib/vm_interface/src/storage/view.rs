use std::{
    cell::RefCell,
    collections::HashMap,
    fmt, mem,
    rc::Rc,
    time::{Duration, Instant},
};

use zksync_types::{StorageKey, StorageValue, H256};

use super::{ReadStorage, StoragePtr, WriteStorage};

/// Statistics for [`StorageView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageViewStats {
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

impl StorageViewStats {
    /// Subtracts two sets of statistics. This can be used to measure increment between these stats and older stats for the same VM.
    pub fn saturating_sub(&self, older: &Self) -> Self {
        Self {
            cache_size: self.cache_size.saturating_sub(older.cache_size),
            storage_invocations_missed: self
                .storage_invocations_missed
                .saturating_sub(older.storage_invocations_missed),
            get_value_storage_invocations: self
                .get_value_storage_invocations
                .saturating_sub(older.get_value_storage_invocations),
            set_value_storage_invocations: self
                .set_value_storage_invocations
                .saturating_sub(older.set_value_storage_invocations),
            time_spent_on_storage_missed: self
                .time_spent_on_storage_missed
                .saturating_sub(older.time_spent_on_storage_missed),
            time_spent_on_get_value: self
                .time_spent_on_get_value
                .saturating_sub(older.time_spent_on_get_value),
            time_spent_on_set_value: self
                .time_spent_on_set_value
                .saturating_sub(older.time_spent_on_set_value),
        }
    }
}

/// `StorageView` is a buffer for `StorageLog`s between storage and transaction execution code.
///
/// In order to commit transactions logs should be submitted to the underlying storage
/// after a transaction is executed.
///
/// When executing transactions as a part of L2 block / L1 batch creation,
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
    cache: StorageViewCache,
    stats: StorageViewStats,
}

/// `StorageViewCache` is a struct for caching storage reads and `contains_key()` checks.
#[derive(Debug, Default, Clone)]
pub struct StorageViewCache {
    // Used purely for caching
    read_storage_keys: HashMap<StorageKey, StorageValue>,
    // Cache for `contains_key()` checks. The cache is only valid within one L1 batch execution.
    initial_writes: HashMap<StorageKey, bool>,
}

impl StorageViewCache {
    /// Returns the read storage keys.
    pub fn read_storage_keys(&self) -> HashMap<StorageKey, StorageValue> {
        self.read_storage_keys.clone()
    }

    /// Returns the initial writes.
    pub fn initial_writes(&self) -> HashMap<StorageKey, bool> {
        self.initial_writes.clone()
    }
}

impl<S> StorageView<S> {
    /// Returns the underlying storage cache.
    pub fn cache(&self) -> StorageViewCache {
        self.cache.clone()
    }

    /// Provides mutable access to the underlying storage.
    ///
    /// # Warning
    ///
    /// Mutating the underlying storage directly can easily break implied `StorageView` invariants, so use with care.
    #[doc(hidden)]
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.storage_handle
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

impl<S: ReadStorage> StorageView<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage_handle: S) -> Self {
        Self {
            storage_handle,
            modified_storage_keys: HashMap::new(),
            cache: StorageViewCache {
                read_storage_keys: HashMap::new(),
                initial_writes: HashMap::new(),
            },
            stats: StorageViewStats::default(),
        }
    }

    fn get_value_no_log(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();

        let cached_value = self
            .modified_storage_keys
            .get(key)
            .or_else(|| self.cache.read_storage_keys.get(key));
        cached_value.copied().unwrap_or_else(|| {
            let value = self.storage_handle.read_value(key);
            self.cache.read_storage_keys.insert(*key, value);
            self.stats.time_spent_on_storage_missed += started_at.elapsed();
            self.stats.storage_invocations_missed += 1;
            value
        })
    }

    fn cache_size(&self) -> usize {
        self.modified_storage_keys.len() * mem::size_of::<(StorageKey, StorageValue)>()
            + self.cache.initial_writes.len() * mem::size_of::<(StorageKey, bool)>()
            + self.cache.read_storage_keys.len() * mem::size_of::<(StorageKey, StorageValue)>()
    }

    /// Returns the current storage access stats.
    pub fn stats(&self) -> StorageViewStats {
        StorageViewStats {
            cache_size: self.cache_size(),
            ..self.stats
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
        self.stats.get_value_storage_invocations += 1;
        let value = self.get_value_no_log(key);

        tracing::trace!(
            "read value {:?} {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value.0,
            key.address(),
            key.key()
        );

        self.stats.time_spent_on_get_value += started_at.elapsed();
        value
    }

    /// Only keys contained in the underlying storage will return `false`. If a key was
    /// inserted using [`Self::set_value()`], it will still return `true`.
    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        if let Some(&is_write_initial) = self.cache.initial_writes.get(key) {
            is_write_initial
        } else {
            let is_write_initial = self.storage_handle.is_write_initial(key);
            self.cache.initial_writes.insert(*key, is_write_initial);
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
    fn read_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.cache.read_storage_keys
    }

    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue {
        let started_at = Instant::now();
        self.stats.set_value_storage_invocations += 1;
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
        self.stats.time_spent_on_set_value += started_at.elapsed();

        original
    }

    fn modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.modified_storage_keys
    }

    fn missed_storage_invocations(&self) -> usize {
        self.stats.storage_invocations_missed
    }
}

/// Immutable wrapper around [`StorageView`] for direct reads.
///
/// Reads directly from the underlying storage ignoring any modifications in the [`StorageView`].
/// Used by the fast VM, which has its own internal management of writes.
#[derive(Debug)]
pub struct ImmutableStorageView<S>(StoragePtr<StorageView<S>>);

impl<S: ReadStorage> ImmutableStorageView<S> {
    /// Creates a new view based on the provided storage pointer.
    pub fn new(ptr: StoragePtr<StorageView<S>>) -> Self {
        Self(ptr)
    }

    #[doc(hidden)] // can easily break invariants if not used carefully
    pub fn to_rc_ptr(&self) -> StoragePtr<StorageView<S>> {
        self.0.clone()
    }
}

// All methods other than `read_value()` do not read back modified storage slots, so we proxy them as-is.
impl<S: ReadStorage> ReadStorage for ImmutableStorageView<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();
        let mut this = self.0.borrow_mut();
        let cached_value = this.read_storage_keys().get(key);
        cached_value.copied().unwrap_or_else(|| {
            let value = this.storage_handle.read_value(key);
            this.cache.read_storage_keys.insert(*key, value);
            this.stats.time_spent_on_storage_missed += started_at.elapsed();
            this.stats.storage_invocations_missed += 1;
            value
        })
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.0.borrow_mut().is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.0.borrow_mut().load_factory_dep(hash)
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.0.borrow_mut().get_enumeration_index(key)
    }
}

#[cfg(test)]
mod test {
    use zksync_types::{AccountTreeId, Address, H256};

    use super::*;
    use crate::storage::InMemoryStorage;

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

        assert_eq!(storage_view.stats().storage_invocations_missed, 1);
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

        let stats = storage_view.stats();
        assert_eq!(stats.storage_invocations_missed, 2);
        assert_eq!(stats.get_value_storage_invocations, 3);
        assert_eq!(stats.set_value_storage_invocations, 2);
    }

    #[test]
    fn immutable_storage_view() {
        let account: AccountTreeId = AccountTreeId::new(Address::from([0xfe; 20]));
        let key = H256::from_low_u64_be(61);
        let value = H256::from_low_u64_be(73);
        let key = StorageKey::new(account, key);

        let mut raw_storage = InMemoryStorage::default();
        raw_storage.set_value(key, value);
        let storage_view = StorageView::new(raw_storage).to_rc_ptr();
        let mut immutable_view = ImmutableStorageView::new(storage_view.clone());

        let new_value = H256::repeat_byte(0x11);
        let prev_value = storage_view.borrow_mut().set_value(key, new_value);
        assert_eq!(prev_value, value);

        assert_eq!(immutable_view.read_value(&key), value);
    }
}

use std::collections::HashMap;

use std::fmt::Debug;
use std::time::Duration;
use zksync_types::{StorageKey, StorageValue, ZkSyncReadStorage, H256};

/// `StorageView` is buffer for `StorageLog`s between storage and transaction execution code.
/// In order to commit transactions logs should be submitted
/// to `ZkSyncStorage` after transaction is executed.
/// Note, you must not use one `StorageView` object for multiple L1 batches,
/// otherwise `is_write_initial` will return incorrect values because of the caching.
#[derive(Debug)]
pub struct StorageView<S> {
    storage_handle: S,
    // Used for caching and to get the list/count of modified keys
    modified_storage_keys: HashMap<StorageKey, StorageValue>,
    // Used purely for caching
    read_storage_keys: HashMap<StorageKey, StorageValue>,
    // Cache for initial/repeated writes. It's only valid within one L1 batch execution.
    read_initial_writes: HashMap<StorageKey, bool>,

    pub storage_invocations_missed: usize,
    pub get_value_storage_invocations: usize,
    pub set_value_storage_invocations: usize,

    pub time_spent_on_storage_missed: Duration,
    pub time_spent_on_get_value: Duration,
    pub time_spent_on_set_value: Duration,
}

impl<S: ZkSyncReadStorage> StorageView<S> {
    pub fn new(storage_handle: S) -> Self {
        Self {
            storage_handle,
            modified_storage_keys: HashMap::new(),
            read_storage_keys: HashMap::new(),
            read_initial_writes: HashMap::new(),
            storage_invocations_missed: 0,
            get_value_storage_invocations: 0,
            set_value_storage_invocations: 0,
            time_spent_on_storage_missed: Default::default(),
            time_spent_on_get_value: Default::default(),
            time_spent_on_set_value: Default::default(),
        }
    }

    pub fn new_with_read_keys(
        storage_handle: S,
        read_storage_keys: HashMap<StorageKey, StorageValue>,
    ) -> Self {
        Self {
            storage_handle,
            modified_storage_keys: HashMap::new(),
            read_storage_keys,
            read_initial_writes: HashMap::new(),
            storage_invocations_missed: 0,
            get_value_storage_invocations: 0,
            set_value_storage_invocations: 0,
            time_spent_on_storage_missed: Default::default(),
            time_spent_on_get_value: Default::default(),
            time_spent_on_set_value: Default::default(),
        }
    }

    pub fn get_value(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = std::time::Instant::now();
        self.get_value_storage_invocations += 1;
        let value = self.get_value_no_log(key);

        vlog::trace!(
            "read value {:?} {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value.0,
            key.address(),
            key.key()
        );

        self.time_spent_on_get_value += started_at.elapsed();
        value
    }

    // returns the value before write. Doesn't generate read logs.
    // `None` for value is only possible for rolling back the transaction
    pub fn set_value(&mut self, key: &StorageKey, value: StorageValue) -> StorageValue {
        let started_at = std::time::Instant::now();
        self.set_value_storage_invocations += 1;

        let original = self.get_value_no_log(key);

        vlog::trace!(
            "write value {:?} value: {:?} original value: {:?} ({:?}/{:?})",
            key.hashed_key().0,
            value,
            original,
            key.address(),
            key.key()
        );
        self.modified_storage_keys.insert(*key, value);

        self.time_spent_on_set_value += started_at.elapsed();

        original
    }

    fn get_value_no_log(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = std::time::Instant::now();

        if let Some(value) = self.modified_storage_keys.get(key) {
            *value
        } else if let Some(value) = self.read_storage_keys.get(key) {
            *value
        } else {
            let value = self.storage_handle.read_value(key);
            self.read_storage_keys.insert(*key, value);

            self.time_spent_on_storage_missed += started_at.elapsed();
            self.storage_invocations_missed += 1;

            value
        }
    }

    pub fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        if let Some(is_initial) = self.read_initial_writes.get(key) {
            *is_initial
        } else {
            let is_initial = self.storage_handle.is_write_initial(key);
            self.read_initial_writes.insert(*key, is_initial);
            is_initial
        }
    }

    pub fn get_modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        &self.modified_storage_keys
    }

    pub fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.storage_handle.load_factory_dep(hash)
    }

    pub fn get_cache_size(&self) -> usize {
        self.modified_storage_keys.len() * std::mem::size_of::<(StorageKey, StorageValue)>()
            + self.read_initial_writes.len() * std::mem::size_of::<(StorageKey, bool)>()
            + self.read_storage_keys.len() * std::mem::size_of::<(StorageKey, StorageValue)>()
    }

    pub fn take_read_storage_keys(self) -> HashMap<StorageKey, StorageValue> {
        self.read_storage_keys
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::secondary_storage::SecondaryStateStorage;
    use tempfile::TempDir;
    use zksync_storage::db::Database;
    use zksync_storage::RocksDB;
    use zksync_types::{AccountTreeId, Address, H256};
    use zksync_utils::u32_to_h256;

    #[test]
    fn test_storage_accessor() {
        let account: AccountTreeId = AccountTreeId::new(Address::from([0xfe; 20]));
        let key = u32_to_h256(61);
        let value = u32_to_h256(73);

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let db = RocksDB::new(Database::StateKeeper, temp_dir.as_ref(), false);
        let raw_storage = SecondaryStateStorage::new(db);

        let mut storage_accessor = StorageView::new(&raw_storage);

        let default_value = storage_accessor.get_value(&StorageKey::new(account, key));
        assert_eq!(default_value, H256::default());

        storage_accessor.set_value(&StorageKey::new(account, key), value);

        let new_value = storage_accessor.get_value(&StorageKey::new(account, key));
        assert_eq!(new_value, value);
    }
}

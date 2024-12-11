//! VM storage functionality specifically used in the VM sandbox.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use zksync_types::{AccountTreeId, StorageKey, StorageValue, H256};

use super::ReadStorage;

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageWithOverrides<S> {
    storage_handle: S,
    overridden_slots: HashMap<StorageKey, H256>,
    overridden_factory_deps: HashMap<H256, Vec<u8>>,
    empty_accounts: HashSet<AccountTreeId>,
}

impl<S: ReadStorage> StorageWithOverrides<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage: S) -> Self {
        Self {
            storage_handle: storage,
            overridden_slots: HashMap::new(),
            overridden_factory_deps: HashMap::new(),
            empty_accounts: HashSet::new(),
        }
    }

    pub fn set_value(&mut self, key: StorageKey, value: StorageValue) {
        self.overridden_slots.insert(key, value);
    }

    pub fn store_factory_dep(&mut self, hash: H256, code: Vec<u8>) {
        self.overridden_factory_deps.insert(hash, code);
    }

    pub fn insert_erased_account(&mut self, account: AccountTreeId) {
        self.empty_accounts.insert(account);
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageWithOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        if let Some(value) = self.overridden_slots.get(key) {
            return *value;
        }
        if self.empty_accounts.contains(key.account()) {
            return H256::zero();
        }
        self.storage_handle.read_value(key)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.storage_handle.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.overridden_factory_deps
            .get(&hash)
            .cloned()
            .or_else(|| self.storage_handle.load_factory_dep(hash))
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.storage_handle.get_enumeration_index(key)
    }
}

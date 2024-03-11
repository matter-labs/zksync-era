use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_types::{AccountTreeId, StorageKey, StorageValue, H256};

use crate::{ReadStorage, StorageView, StorageViewMetrics, WriteStorage};

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageOverrides<S> {
    storage_view: StorageView<S>,
    overrided_factory_deps: HashMap<H256, Vec<u8>>,
    overrided_account_state: HashMap<AccountTreeId, HashMap<H256, H256>>,
}

impl<S: ReadStorage + fmt::Debug> StorageOverrides<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage_view: StorageView<S>) -> Self {
        Self {
            storage_view,
            overrided_factory_deps: HashMap::new(),
            overrided_account_state: HashMap::new(),
        }
    }

    /// Overrides a factory dep code.
    pub fn store_factory_dep(&mut self, hash: H256, code: Vec<u8>) {
        self.overrided_factory_deps.insert(hash, code);
    }

    /// Overrides an account entire state.
    pub fn override_account_state(&mut self, account: AccountTreeId, state: HashMap<H256, H256>) {
        self.overrided_account_state.insert(account, state);
    }

    /// Returns the current metrics.
    pub fn metrics(&self) -> StorageViewMetrics {
        self.storage_view.metrics()
    }

    /// Make a Rc RefCell ptr to the storage
    pub fn to_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        // If the account is overrided, return the overrided value if any or zero.
        // Otherwise, return the value from the underlying storage.
        self.overrided_account_state
            .get(key.account())
            .and_then(|state| match state.get(key.key()) {
                Some(v) => Some(v.clone()),
                None => Some(H256::zero()),
            })
            .unwrap_or_else(|| self.storage_view.read_value(key))
    }

    /// Only keys contained in the underlying storage will return `false`. If a key was
    /// inserted using [`Self::set_value()`], it will still return `true`.
    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.storage_view.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.overrided_factory_deps
            .get(&hash)
            .cloned()
            .or_else(|| self.storage_view.load_factory_dep(hash))
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.storage_view.get_enumeration_index(key)
    }
}

impl<S: ReadStorage + fmt::Debug> WriteStorage for StorageOverrides<S> {
    fn set_value(&mut self, key: StorageKey, value: StorageValue) -> StorageValue {
        self.storage_view.set_value(key, value)
    }

    fn modified_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        self.storage_view.modified_storage_keys()
    }

    fn missed_storage_invocations(&self) -> usize {
        self.storage_view.missed_storage_invocations()
    }
}

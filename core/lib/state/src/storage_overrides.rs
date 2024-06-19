use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_types::{AccountTreeId, StorageKey, StorageValue, H256, U256};
use zksync_utils::u256_to_h256;

use crate::ReadStorage;

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageOverrides<S> {
    storage_handle: S,
    overrided_factory_deps: HashMap<H256, Vec<u8>>,
    overrided_account_state: HashMap<AccountTreeId, HashMap<H256, H256>>,
    overrided_balance: HashMap<StorageKey, U256>,
    overrided_nonce: HashMap<StorageKey, U256>,
    overrided_code: HashMap<StorageKey, H256>,
}

impl<S: ReadStorage + fmt::Debug> StorageOverrides<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage: S) -> Self {
        Self {
            storage_handle: storage,
            overrided_factory_deps: HashMap::new(),
            overrided_account_state: HashMap::new(),
            overrided_balance: HashMap::new(),
            overrided_nonce: HashMap::new(),
            overrided_code: HashMap::new(),
        }
    }

    /// Overrides a factory dependency code.
    pub fn store_factory_dep(&mut self, hash: H256, code: Vec<u8>) {
        self.overrided_factory_deps.insert(hash, code);
    }

    /// Overrides an account entire state.
    pub fn override_account_state(&mut self, account: AccountTreeId, state: HashMap<H256, H256>) {
        self.overrided_account_state.insert(account, state);
    }

    /// Overrides an account balance.
    pub fn store_balance_override(&mut self, key: StorageKey, balance: U256) {
        self.overrided_balance.insert(key, balance);
    }

    /// Overrides an account nonce.
    pub fn store_nonce_override(&mut self, key: StorageKey, nonce: U256) {
        self.overrided_nonce.insert(key, nonce);
    }

    /// Overrides an account code.
    pub fn store_code_overrrided(&mut self, key: StorageKey, code: H256) {
        self.overrided_code.insert(key, code);
    }

    /// Make a Rc RefCell ptr to the storage
    pub fn to_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        if let Some(balance) = self.overrided_balance.get(key) {
            return u256_to_h256(*balance);
        }
        if let Some(code) = self.overrided_code.get(key) {
            return *code;
        }

        if let Some(nonce) = self.overrided_nonce.get(key) {
            return u256_to_h256(*nonce);
        }

        self.overrided_account_state.get(key.account()).map_or_else(
            || self.storage_handle.read_value(key),
            |state| state.get(key.key()).copied().unwrap_or_else(H256::zero),
        )
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.storage_handle.is_write_initial(key)
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.overrided_factory_deps
            .get(&hash)
            .cloned()
            .or_else(|| self.storage_handle.load_factory_dep(hash))
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        self.storage_handle.get_enumeration_index(key)
    }
}

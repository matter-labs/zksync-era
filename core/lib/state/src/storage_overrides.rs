use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_types::{
    api::state_override::{OverrideState, StateOverride},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    AccountTreeId, StorageKey, StorageValue, H256,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use crate::{ReadStorage, StorageView, StorageViewMetrics, WriteStorage};

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageOverrides<S> {
    storage_view: StorageView<S>,
    overrided_factory_deps: HashMap<H256, Vec<u8>>,
    overrided_account_state: HashMap<AccountTreeId, HashMap<H256, H256>>,
}

impl<S: ReadStorage + fmt::Debug> StorageOverrides<S> {
    /// Applies the state override to the storage.
    pub fn apply_state_override(&mut self, state_override: &StateOverride) {
        for (account, overrides) in state_override.iter() {
            if let Some(balance) = overrides.balance {
                let balance_key = storage_key_for_eth_balance(account);
                self.set_value(balance_key, u256_to_h256(balance));
            }

            if let Some(nonce) = overrides.nonce {
                let nonce_key = get_nonce_key(account);

                let full_nonce = self.read_value(&nonce_key);
                let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
                let new_full_nonce = nonces_to_full_nonce(nonce, deployment_nonce);

                self.set_value(nonce_key, u256_to_h256(new_full_nonce));
            }

            if let Some(code) = &overrides.code {
                let code_key = get_code_key(account);
                let code_hash = hash_bytecode(&code.0);

                self.set_value(code_key, code_hash);
                self.store_factory_dep(code_hash, code.0.clone());
            }

            match &overrides.state {
                Some(OverrideState::State(state)) => {
                    self.override_account_state(AccountTreeId::new(*account), state.clone());
                }
                Some(OverrideState::StateDiff(state_diff)) => {
                    for (key, value) in state_diff {
                        self.set_value(StorageKey::new(AccountTreeId::new(*account), *key), *value);
                    }
                }
                None => {}
            }
        }
    }
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

    /// Overrides a factory dependency code.
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

impl<S: ReadStorage + fmt::Debug> From<StorageOverrides<S>> for Rc<RefCell<StorageOverrides<S>>> {
    fn from(storage_overrides: StorageOverrides<S>) -> Rc<RefCell<StorageOverrides<S>>> {
        storage_overrides.to_rc_ptr()
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        // If the account is overridden, return the overridden value if any or zero.
        // Otherwise, return the value from the underlying storage.
        self.overrided_account_state.get(key.account()).map_or_else(
            || self.storage_view.read_value(key),
            |state| state.get(key.key()).copied().unwrap_or_else(H256::zero),
        )
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

    fn read_storage_keys(&self) -> &HashMap<StorageKey, StorageValue> {
        self.storage_view.read_storage_keys()
    }
}

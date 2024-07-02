use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use zksync_types::{
    api::state_override::{OverrideState, StateOverride},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    AccountTreeId, StorageKey, StorageValue, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use crate::{OverrideStorage, ReadStorage};

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageOverrides<S> {
    storage_handle: S,
    overridden_factory_deps: HashMap<H256, Vec<u8>>,
    overridden_account_state: HashMap<AccountTreeId, HashMap<H256, H256>>,
    overriden_account_state_diff: HashMap<AccountTreeId, HashMap<H256, H256>>,
    overridden_balance: HashMap<StorageKey, U256>,
    overridden_nonce: HashMap<StorageKey, U256>,
    overridden_code: HashMap<StorageKey, H256>,
}

impl<S: ReadStorage + fmt::Debug> StorageOverrides<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage: S) -> Self {
        Self {
            storage_handle: storage,
            overridden_factory_deps: HashMap::new(),
            overridden_account_state: HashMap::new(),
            overriden_account_state_diff: HashMap::new(),
            overridden_balance: HashMap::new(),
            overridden_nonce: HashMap::new(),
            overridden_code: HashMap::new(),
        }
    }

    /// Overrides a factory dependency code.
    pub fn store_factory_dep(&mut self, hash: H256, code: Vec<u8>) {
        self.overridden_factory_deps.insert(hash, code);
    }

    /// Overrides an account entire state.
    pub fn override_account_state(&mut self, account: AccountTreeId, state: HashMap<H256, H256>) {
        self.overridden_account_state.insert(account, state);
    }

    /// Overrides an account state diff.
    pub fn override_account_state_diff(
        &mut self,
        account: AccountTreeId,
        state_diff: HashMap<H256, H256>,
    ) {
        self.overriden_account_state_diff
            .insert(account, state_diff);
    }

    /// Make a Rc RefCell ptr to the storage
    pub fn to_rc_ptr(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        if let Some(balance) = self.overridden_balance.get(key) {
            return u256_to_h256(*balance);
        }
        if let Some(code) = self.overridden_code.get(key) {
            return *code;
        }

        if let Some(nonce) = self.overridden_nonce.get(key) {
            return u256_to_h256(*nonce);
        }

        if let Some(account_state) = self.overridden_account_state.get(key.account()) {
            if let Some(value) = account_state.get(key.key()) {
                return *value;
            }
        }

        if let Some(account_state_diff) = self.overriden_account_state_diff.get(key.account()) {
            if let Some(value) = account_state_diff.get(key.key()) {
                return *value;
            }
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

impl<S: ReadStorage + fmt::Debug> OverrideStorage for StorageOverrides<S> {
    fn apply_state_override(&mut self, state_override: &StateOverride) {
        for (account, overrides) in state_override.iter() {
            if let Some(balance) = overrides.balance {
                let balance_key = storage_key_for_eth_balance(account);
                self.overridden_balance.insert(balance_key, balance);
            }

            if let Some(nonce) = overrides.nonce {
                let nonce_key = get_nonce_key(account);
                let full_nonce = self.read_value(&nonce_key);
                let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
                let new_full_nonce = nonces_to_full_nonce(nonce, deployment_nonce);
                self.overridden_nonce.insert(nonce_key, new_full_nonce);
            }

            if let Some(code) = &overrides.code {
                let code_key = get_code_key(account);
                let code_hash = hash_bytecode(&code.0);
                self.overridden_code.insert(code_key, code_hash);
                self.store_factory_dep(code_hash, code.0.clone());
            }

            match &overrides.state {
                Some(OverrideState::State(state)) => {
                    self.override_account_state(AccountTreeId::new(*account), state.clone());
                }
                Some(OverrideState::StateDiff(state_diff)) => {
                    for (key, value) in state_diff {
                        let account_state = self
                            .overridden_account_state
                            .entry(AccountTreeId::new(*account))
                            .or_default();
                        account_state.insert(*key, *value);
                    }
                }
                None => {}
            }
        }
    }
}

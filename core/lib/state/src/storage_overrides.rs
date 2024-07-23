use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use zksync_types::{
    api::state_override::{OverrideState, StateOverride},
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    AccountTreeId, StorageKey, StorageValue, H256,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use crate::ReadStorage;

/// A storage view that allows to override some of the storage values.
#[derive(Debug)]
pub struct StorageWithOverrides<S> {
    storage_handle: S,
    overridden_slots: HashMap<StorageKey, H256>,
    overridden_factory_deps: HashMap<H256, Vec<u8>>,
    overridden_accounts: HashSet<AccountTreeId>,
}

impl<S: ReadStorage> StorageWithOverrides<S> {
    /// Creates a new storage view based on the underlying storage.
    pub fn new(storage: S, state_override: &StateOverride) -> Self {
        let mut this = Self {
            storage_handle: storage,
            overridden_slots: HashMap::new(),
            overridden_factory_deps: HashMap::new(),
            overridden_accounts: HashSet::new(),
        };
        this.apply_state_override(state_override);
        this
    }

    fn apply_state_override(&mut self, state_override: &StateOverride) {
        for (account, overrides) in state_override.iter() {
            if let Some(balance) = overrides.balance {
                let balance_key = storage_key_for_eth_balance(account);
                self.overridden_slots
                    .insert(balance_key, u256_to_h256(balance));
            }

            if let Some(nonce) = overrides.nonce {
                let nonce_key = get_nonce_key(account);
                let full_nonce = self.read_value(&nonce_key);
                let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
                let new_full_nonce = u256_to_h256(nonces_to_full_nonce(nonce, deployment_nonce));
                self.overridden_slots.insert(nonce_key, new_full_nonce);
            }

            // FIXME: will panic if the code is not aligned
            if let Some(code) = &overrides.code {
                let code_key = get_code_key(account);
                let code_hash = hash_bytecode(&code.0);
                self.overridden_slots.insert(code_key, code_hash);
                self.store_factory_dep(code_hash, code.0.clone());
            }

            match &overrides.state {
                Some(OverrideState::State(state)) => {
                    let account = AccountTreeId::new(*account);
                    self.override_account_state_diff(account, state);
                    self.overridden_accounts.insert(account);
                }
                Some(OverrideState::StateDiff(state_diff)) => {
                    let account = AccountTreeId::new(*account);
                    self.override_account_state_diff(account, state_diff);
                }
                None => { /* do nothing */ }
            }
        }
    }

    fn store_factory_dep(&mut self, hash: H256, code: Vec<u8>) {
        self.overridden_factory_deps.insert(hash, code);
    }

    fn override_account_state_diff(
        &mut self,
        account: AccountTreeId,
        state_diff: &HashMap<H256, H256>,
    ) {
        let account_slots = state_diff
            .iter()
            .map(|(&slot, &value)| (StorageKey::new(account, slot), value));
        self.overridden_slots.extend(account_slots);
    }
}

impl<S: ReadStorage + fmt::Debug> ReadStorage for StorageWithOverrides<S> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        if let Some(value) = self.overridden_slots.get(key) {
            return *value;
        }
        if self.overridden_accounts.contains(key.account()) {
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

#[cfg(test)]
mod tests {
    use zksync_types::{api::state_override::OverrideAccount, web3::Bytes, Address};

    use super::*;
    use crate::InMemoryStorage;

    #[test]
    fn override_basics() {
        let overrides = StateOverride::new(HashMap::from([
            (
                Address::repeat_byte(1),
                OverrideAccount {
                    balance: Some(1.into()),
                    ..OverrideAccount::default()
                },
            ),
            (
                Address::repeat_byte(2),
                OverrideAccount {
                    nonce: Some(2.into()),
                    ..OverrideAccount::default()
                },
            ),
            (
                Address::repeat_byte(3),
                OverrideAccount {
                    code: Some(Bytes((0..32).collect())),
                    ..OverrideAccount::default()
                },
            ),
            (
                Address::repeat_byte(4),
                OverrideAccount {
                    state: Some(OverrideState::StateDiff(HashMap::from([(
                        H256::zero(),
                        H256::repeat_byte(1),
                    )]))),
                    ..OverrideAccount::default()
                },
            ),
            (
                Address::repeat_byte(5),
                OverrideAccount {
                    state: Some(OverrideState::State(HashMap::new())),
                    ..OverrideAccount::default()
                },
            ),
        ]));

        let mut storage = InMemoryStorage::default();
        let overridden_key =
            StorageKey::new(AccountTreeId::new(Address::repeat_byte(4)), H256::zero());
        storage.set_value(overridden_key, H256::repeat_byte(0xff));
        let retained_key = StorageKey::new(
            AccountTreeId::new(Address::repeat_byte(4)),
            H256::from_low_u64_be(1),
        );
        storage.set_value(retained_key, H256::repeat_byte(0xfe));
        let erased_key = StorageKey::new(AccountTreeId::new(Address::repeat_byte(5)), H256::zero());
        storage.set_value(erased_key, H256::repeat_byte(1));
        let mut storage = StorageWithOverrides::new(storage, &overrides);

        let balance = storage.read_value(&storage_key_for_eth_balance(&Address::repeat_byte(1)));
        assert_eq!(balance, H256::from_low_u64_be(1));
        let nonce = storage.read_value(&get_nonce_key(&Address::repeat_byte(2)));
        assert_eq!(nonce, H256::from_low_u64_be(2));
        let code_hash = storage.read_value(&get_code_key(&Address::repeat_byte(3)));
        assert_ne!(code_hash, H256::zero());
        assert!(storage.load_factory_dep(code_hash).is_some());

        let overridden_value = storage.read_value(&overridden_key);
        assert_eq!(overridden_value, H256::repeat_byte(1));
        let retained_value = storage.read_value(&retained_key);
        assert_eq!(retained_value, H256::repeat_byte(0xfe));
        let erased_value = storage.read_value(&erased_key);
        assert_eq!(erased_value, H256::zero());
    }
}

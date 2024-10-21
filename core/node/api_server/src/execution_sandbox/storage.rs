//! VM storage functionality specifically used in the VM sandbox.

use zksync_multivm::interface::storage::{ReadStorage, StorageWithOverrides};
use zksync_types::{
    api::state_override::{OverrideState, StateOverride},
    get_code_key, get_known_code_key, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    AccountTreeId, StorageKey, H256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

/// This method is blocking.
pub(super) fn apply_state_override<S: ReadStorage>(
    storage: S,
    state_override: &StateOverride,
) -> StorageWithOverrides<S> {
    let mut storage = StorageWithOverrides::new(storage);
    for (account, overrides) in state_override.iter() {
        if let Some(balance) = overrides.balance {
            let balance_key = storage_key_for_eth_balance(account);
            storage.set_value(balance_key, u256_to_h256(balance));
        }

        if let Some(nonce) = overrides.nonce {
            let nonce_key = get_nonce_key(account);
            let full_nonce = storage.read_value(&nonce_key);
            let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
            let new_full_nonce = u256_to_h256(nonces_to_full_nonce(nonce, deployment_nonce));
            storage.set_value(nonce_key, new_full_nonce);
        }

        if let Some(code) = &overrides.code {
            let code_key = get_code_key(account);
            let code_hash = code.hash();
            storage.set_value(code_key, code_hash);
            let known_code_key = get_known_code_key(&code_hash);
            storage.set_value(known_code_key, H256::from_low_u64_be(1));
            storage.store_factory_dep(code_hash, code.clone().into_bytes());
        }

        match &overrides.state {
            Some(OverrideState::State(state)) => {
                let account = AccountTreeId::new(*account);
                for (&key, &value) in state {
                    storage.set_value(StorageKey::new(account, key), value);
                }
                storage.insert_erased_account(account);
            }
            Some(OverrideState::StateDiff(state_diff)) => {
                let account = AccountTreeId::new(*account);
                for (&key, &value) in state_diff {
                    storage.set_value(StorageKey::new(account, key), value);
                }
            }
            None => { /* do nothing */ }
        }
    }
    storage
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zksync_multivm::interface::storage::InMemoryStorage;
    use zksync_types::{
        api::state_override::{Bytecode, OverrideAccount},
        Address,
    };

    use super::*;

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
                    code: Some(Bytecode::new((0..32).collect()).unwrap()),
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
        let mut storage = apply_state_override(storage, &overrides);

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

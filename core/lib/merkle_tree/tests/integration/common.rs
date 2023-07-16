//! Shared functionality.

use zksync_types::{AccountTreeId, Address, StorageKey, H256, U256};

pub fn generate_key_value_pairs(indexes: impl Iterator<Item = u64>) -> Vec<(U256, H256)> {
    let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
    let kvs = indexes.map(|idx| {
        let key = H256::from_low_u64_be(idx);
        let key = StorageKey::new(AccountTreeId::new(address), key);
        (key.hashed_key_u256(), H256::from_low_u64_be(idx + 1))
    });
    kvs.collect()
}

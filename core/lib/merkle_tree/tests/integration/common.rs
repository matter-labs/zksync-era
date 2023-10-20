//! Shared functionality.

use once_cell::sync::Lazy;

use std::collections::HashMap;

use zksync_crypto::hasher::{blake2::Blake2Hasher, Hasher};
use zksync_merkle_tree::{HashTree, TreeInstruction};
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

pub fn compute_tree_hash(kvs: impl Iterator<Item = (U256, H256)>) -> H256 {
    let kvs_with_indices = kvs
        .enumerate()
        .map(|(i, (key, value))| (key, value, i as u64 + 1));
    compute_tree_hash_with_indices(kvs_with_indices)
}

// The extended version of computations used in `InternalNode`.
fn compute_tree_hash_with_indices(kvs: impl Iterator<Item = (U256, H256, u64)>) -> H256 {
    let hasher = Blake2Hasher;
    let mut empty_tree_hash = hasher.hash_bytes(&[0_u8; 40]);
    let level = kvs.map(|(key, value, leaf_index)| {
        let mut bytes = [0_u8; 40];
        bytes[..8].copy_from_slice(&leaf_index.to_be_bytes());
        bytes[8..].copy_from_slice(value.as_ref());
        (key, hasher.hash_bytes(&bytes))
    });
    let mut level: Vec<(U256, H256)> = level.collect();
    if level.is_empty() {
        return hasher.empty_subtree_hash(256);
    }
    level.sort_unstable_by_key(|(key, _)| *key);

    for _ in 0..256 {
        let mut next_level = vec![];
        let mut i = 0;
        while i < level.len() {
            let (pos, hash) = level[i];
            let aggregate_hash = if pos.bit(0) {
                // `pos` corresponds to a right branch of its parent
                hasher.compress(&empty_tree_hash, &hash)
            } else if let Some((next_pos, next_hash)) = level.get(i + 1) {
                if pos + 1 == *next_pos {
                    i += 1;
                    hasher.compress(&hash, next_hash)
                } else {
                    hasher.compress(&hash, &empty_tree_hash)
                }
            } else {
                hasher.compress(&hash, &empty_tree_hash)
            };
            next_level.push((pos >> 1, aggregate_hash));
            i += 1;
        }

        level = next_level;
        empty_tree_hash = hasher.compress(&empty_tree_hash, &empty_tree_hash);
    }
    level[0].1
}

// Computing the expected hash takes some time in the debug mode, so we memoize it.
pub static KVS_AND_HASH: Lazy<(Vec<(U256, H256)>, H256)> = Lazy::new(|| {
    let kvs = generate_key_value_pairs(0..100);
    let expected_hash = compute_tree_hash(kvs.iter().copied());
    (kvs, expected_hash)
});

pub fn convert_to_writes(kvs: &[(U256, H256)]) -> Vec<(U256, TreeInstruction)> {
    let kvs = kvs
        .iter()
        .map(|&(key, hash)| (key, TreeInstruction::Write(hash)));
    kvs.collect()
}

/// Emulates leaf index assignment in a real Merkle tree.
#[derive(Debug)]
pub struct TreeMap(HashMap<U256, (H256, u64)>);

impl TreeMap {
    pub fn new(initial_entries: &[(U256, H256)]) -> Self {
        let map = initial_entries
            .iter()
            .enumerate()
            .map(|(i, (key, value))| (*key, (*value, i as u64 + 1)))
            .collect();
        Self(map)
    }

    pub fn extend(&mut self, kvs: &[(U256, H256)]) {
        for &(key, new_value) in kvs {
            if let Some((value, _)) = self.0.get_mut(&key) {
                *value = new_value;
            } else {
                let leaf_index = self.0.len() as u64 + 1;
                self.0.insert(key, (new_value, leaf_index));
            }
        }
    }

    pub fn root_hash(&self) -> H256 {
        let entries = self
            .0
            .iter()
            .map(|(key, (value, idx))| (*key, *value, *idx));
        compute_tree_hash_with_indices(entries)
    }
}

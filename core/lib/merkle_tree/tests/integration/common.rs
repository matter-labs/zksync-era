//! Shared functionality.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};
use zksync_merkle_tree::{HashTree, TreeEntry, TreeInstruction};
use zksync_types::{AccountTreeId, Address, StorageKey, H256, U256};

pub fn generate_key_value_pairs(indexes: impl Iterator<Item = u64>) -> Vec<TreeEntry> {
    let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
    let kvs = indexes.map(|idx| {
        let key = H256::from_low_u64_be(idx);
        let key = StorageKey::new(AccountTreeId::new(address), key);
        let value = H256::from_low_u64_be(idx + 1);
        TreeEntry::new(key.hashed_key_u256(), idx + 1, value)
    });
    kvs.collect()
}

pub fn compute_tree_hash(kvs: impl Iterator<Item = TreeEntry>) -> H256 {
    let kvs_with_indices = kvs.map(|entry| (entry.key, entry.value, entry.leaf_index));
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
pub static ENTRIES_AND_HASH: Lazy<(Vec<TreeEntry>, H256)> = Lazy::new(|| {
    let entries = generate_key_value_pairs(0..100);
    let expected_hash = compute_tree_hash(entries.iter().copied());
    (entries, expected_hash)
});

pub fn convert_to_writes(entries: &[TreeEntry]) -> Vec<TreeInstruction> {
    entries
        .iter()
        .copied()
        .map(TreeInstruction::Write)
        .collect()
}

/// Emulates leaf index assignment in a real Merkle tree.
#[derive(Debug)]
pub struct TreeMap(HashMap<U256, (H256, u64)>);

impl TreeMap {
    pub fn new(initial_entries: &[TreeEntry]) -> Self {
        let map = initial_entries
            .iter()
            .map(|entry| (entry.key, (entry.value, entry.leaf_index)))
            .collect();
        Self(map)
    }

    pub fn extend(&mut self, kvs: &[TreeEntry]) {
        for &new_entry in kvs {
            if let Some((value, leaf_index)) = self.0.get_mut(&new_entry.key) {
                assert_eq!(*leaf_index, new_entry.leaf_index); // sanity check
                *value = new_entry.value;
            } else {
                self.0
                    .insert(new_entry.key, (new_entry.value, new_entry.leaf_index));
            }
        }
    }

    pub fn root_hash(&self) -> H256 {
        let entries = self
            .0
            .iter()
            .map(|(key, (value, leaf_index))| (*key, *value, *leaf_index));
        compute_tree_hash_with_indices(entries)
    }
}

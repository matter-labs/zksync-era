//! Tests for the public `MerkleTree` interface.

use std::collections::{BTreeMap, HashMap};

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use super::*;
use crate::{storage::PatchSet, types::Leaf};

fn naive_hash_tree(entries: &[TreeEntry]) -> H256 {
    let mut indices = BTreeMap::from([(H256::zero(), 0_u64), (H256::repeat_byte(0xff), 1)]);
    indices.extend(
        entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.key, i as u64 + 2)),
    );
    let prev_indices: Vec<_> = [0].into_iter().chain(indices.values().copied()).collect();
    let next_indices: Vec<_> = indices.values().skip(1).copied().chain([1]).collect();
    let prev_and_next_indices: HashMap<_, _> = indices
        .into_keys()
        .zip(prev_indices.into_iter().zip(next_indices))
        .collect();

    let leaves = [&TreeEntry::MIN_GUARD, &TreeEntry::MAX_GUARD]
        .into_iter()
        .chain(entries)
        .map(|entry| {
            let (prev_index, next_index) = prev_and_next_indices[&entry.key];
            Leaf {
                key: entry.key,
                value: entry.value,
                prev_index,
                next_index,
            }
        });

    let mut hashes: Vec<_> = leaves.map(|leaf| Blake2Hasher.hash_leaf(&leaf)).collect();
    for depth in 0..64 {
        if hashes.len() % 2 == 1 {
            hashes.push(Blake2Hasher.empty_subtree_hash(depth));
        }
        hashes = hashes
            .chunks(2)
            .map(|chunk| match chunk {
                [lhs, rhs] => Blake2Hasher.hash_branch(lhs, rhs),
                _ => unreachable!(),
            })
            .collect();
    }
    hashes[0]
}

#[test]
fn comparing_tree_hash_against_naive_impl() {
    const RNG_SEED: u64 = 42;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..100).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();
    let expected_root_hash = naive_hash_tree(&inserts);

    for chunk_size in [1, 2, 3, 5, 8, 13, 21, 34, 55, 100] {
        println!("Insert in {chunk_size}-sized chunks");
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        for chunk in inserts.chunks(chunk_size) {
            tree.extend(chunk.to_vec()).unwrap();
        }
        let root_hash = tree.latest_root_hash().unwrap().expect("tree is empty");
        assert_eq!(root_hash, expected_root_hash);
    }
}

#[test]
fn comparing_tree_hash_with_updates() {
    const RNG_SEED: u64 = 42;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..100).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();
    let initial_root_hash = naive_hash_tree(&inserts);

    let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
    tree.extend(inserts.clone()).unwrap();
    assert_eq!(
        tree.latest_root_hash().unwrap().expect("tree is empty"),
        initial_root_hash
    );
    let initial_patch = tree.db;

    let mut updates = inserts;
    for update in &mut updates {
        update.value = H256(rng.gen());
    }
    let new_root_hash = naive_hash_tree(&updates);
    updates.shuffle(&mut rng);

    for chunk_size in [1, 2, 3, 5, 8, 13, 21, 34, 55, 100] {
        println!("Update in {chunk_size}-sized chunks");
        let mut tree = MerkleTree::new(initial_patch.clone()).unwrap();
        for chunk in updates.chunks(chunk_size) {
            tree.extend(chunk.to_vec()).unwrap();
        }
        let root_hash = tree.latest_root_hash().unwrap().expect("tree is empty");
        assert_eq!(root_hash, new_root_hash);
    }
}

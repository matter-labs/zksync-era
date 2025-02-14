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

fn test_comparing_tree_hash_against_naive_impl<DB: Database>(mut create_db: impl FnMut() -> DB) {
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
        let mut tree = MerkleTree::new(create_db()).unwrap();
        for chunk in inserts.chunks(chunk_size) {
            tree.extend(chunk).unwrap();
        }
        let root_hash = tree.latest_root_hash().unwrap().expect("tree is empty");
        assert_eq!(root_hash, expected_root_hash);

        let latest_version = tree.latest_version().unwrap().expect("no version");
        for version in 0..=latest_version {
            println!("Verifying version {version}");
            tree.verify_consistency(version).unwrap();
        }
    }
}

#[test]
fn comparing_tree_hash_against_naive_impl() {
    test_comparing_tree_hash_against_naive_impl(PatchSet::default);
}

fn test_comparing_tree_hash_with_updates(db: impl Database) {
    const RNG_SEED: u64 = 42;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..100).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();
    let initial_root_hash = naive_hash_tree(&inserts);

    let mut tree = MerkleTree::new(db).unwrap();
    tree.extend(&inserts).unwrap();
    assert_eq!(
        tree.latest_root_hash().unwrap().expect("tree is empty"),
        initial_root_hash
    );

    let mut updates = inserts;
    for update in &mut updates {
        update.value = H256(rng.gen());
    }
    let new_root_hash = naive_hash_tree(&updates);
    updates.shuffle(&mut rng);

    for chunk_size in [1, 2, 3, 5, 8, 13, 21, 34, 55, 100] {
        println!("Update in {chunk_size}-sized chunks");
        for chunk in updates.chunks(chunk_size) {
            tree.extend(chunk).unwrap();
        }
        let root_hash = tree.latest_root_hash().unwrap().expect("tree is empty");
        assert_eq!(root_hash, new_root_hash);

        let latest_version = tree.latest_version().unwrap().expect("no version");
        for version in 0..=latest_version {
            println!("Verifying version {version}");
            tree.verify_consistency(version).unwrap();
        }

        tree.truncate_recent_versions(1).unwrap();
        assert_eq!(tree.latest_version().unwrap(), Some(0));
        assert_eq!(tree.latest_root_hash().unwrap(), Some(initial_root_hash));
    }
}

#[test]
fn comparing_tree_hash_with_updates() {
    test_comparing_tree_hash_with_updates(PatchSet::default());
}

mod rocksdb {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn comparing_tree_hash_against_naive_impl() {
        let temp_dir = TempDir::new().unwrap();
        let mut i = 0;
        test_comparing_tree_hash_against_naive_impl(|| {
            i += 1;
            RocksDBWrapper::new(&temp_dir.path().join(i.to_string())).unwrap()
        });
    }

    #[test]
    fn comparing_tree_hash_with_updates() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        test_comparing_tree_hash_with_updates(db);
    }
}

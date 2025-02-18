//! Tests for the public `MerkleTree` interface.

use std::collections::{BTreeMap, HashMap};

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use super::*;
use crate::{
    hasher::TreeOperation,
    storage::PatchSet,
    types::{Leaf, TreeTags},
};

#[test]
fn tree_depth_mismatch() {
    let mut db = PatchSet::default();
    db.manifest_mut().version_count = 1;
    db.manifest_mut().tags = TreeTags {
        depth: 48,
        ..TreeTags::for_params::<DefaultTreeParams>(&Blake2Hasher)
    };

    let err = MerkleTree::new(db).unwrap_err().to_string();
    assert!(
        err.contains("Unexpected tree depth: expected 64, got 48"),
        "{err}"
    );
}

#[test]
fn tree_internal_node_depth_mismatch() {
    let mut db = PatchSet::default();
    db.manifest_mut().version_count = 1;
    db.manifest_mut().tags = TreeTags {
        internal_node_depth: 3,
        ..TreeTags::for_params::<DefaultTreeParams>(&Blake2Hasher)
    };

    let err = MerkleTree::new(db).unwrap_err().to_string();
    assert!(
        err.contains("Unexpected internal node depth: expected 4, got 3"),
        "{err}"
    );
}

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

fn test_extending_tree_with_proof(db: impl Database, inserts_count: usize, update_count: usize) {
    const RNG_SEED: u64 = 42;

    let mut tree = MerkleTree::new(db).unwrap();
    let empty_tree_hash = tree.extend(&[]).unwrap().root_hash;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..inserts_count).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();

    let (output, proof) = tree.extend_with_proof(&inserts).unwrap();
    let inserts_tree_hash = output.root_hash;

    assert_eq!(proof.operations.len(), inserts.len());
    for op in &proof.operations {
        assert_eq!(*op, TreeOperation::Insert { prev_index: 0 });
    }
    assert_eq!(proof.sorted_leaves.len(), 2);
    assert_eq!(
        proof.sorted_leaves.keys().copied().collect::<Vec<_>>(),
        [0, 1]
    );

    let root_hash_from_proof = proof
        .verify(&Blake2Hasher, 64, &inserts, 2, empty_tree_hash)
        .unwrap();
    assert_eq!(root_hash_from_proof, inserts_tree_hash);

    // Test a proof with only updates.
    let updates: Vec<_> = inserts
        .choose_multiple(&mut rng, update_count)
        .map(|entry| TreeEntry {
            key: entry.key,
            value: H256::zero(),
        })
        .collect();

    let (output, proof) = tree.extend_with_proof(&updates).unwrap();
    let updates_tree_hash = output.root_hash;

    assert_eq!(proof.operations.len(), updates.len());
    let mut updated_indices = vec![];
    for op in &proof.operations {
        match *op {
            TreeOperation::Update { index } => updated_indices.push(index),
            TreeOperation::Insert { .. } => panic!("unexpected operation: {op:?}"),
        }
    }
    updated_indices.sort_unstable();

    assert_eq!(proof.sorted_leaves.len(), updates.len());
    assert_eq!(
        proof.sorted_leaves.keys().copied().collect::<Vec<_>>(),
        updated_indices
    );

    let root_hash_from_proof = proof
        .verify(
            &Blake2Hasher,
            64,
            &updates,
            2 + inserts.len() as u64,
            inserts_tree_hash,
        )
        .unwrap();
    assert_eq!(root_hash_from_proof, updates_tree_hash);
}

#[test]
fn extending_tree_with_proof() {
    for insert_count in [10, 20, 50, 100, 1_000] {
        for update_count in [1, 2, insert_count / 4, insert_count / 2] {
            println!("insert_count={insert_count}, update_count={update_count}");
            test_extending_tree_with_proof(PatchSet::default(), insert_count, update_count);
        }
    }
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

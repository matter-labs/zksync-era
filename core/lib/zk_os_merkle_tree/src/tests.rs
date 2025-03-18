//! Tests for the public `MerkleTree` interface.

use std::collections::{BTreeMap, HashMap, HashSet};

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use super::*;
use crate::{
    hasher::TreeOperation,
    storage::{PatchSet, Patched},
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
        internal_node_depth: 2,
        ..TreeTags::for_params::<DefaultTreeParams>(&Blake2Hasher)
    };

    let err = MerkleTree::new(db).unwrap_err().to_string();
    assert!(
        err.contains("Unexpected internal node depth: expected 3, got 2"),
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

fn test_extending_tree_with_proof(db: impl Database, inserts_count: usize, updates_count: usize) {
    const RNG_SEED: u64 = 42;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..inserts_count).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();

    let missing_reads: Vec<_> = (0..inserts_count).map(|_| H256(rng.gen())).collect();

    let mut tree = MerkleTree::new(db).unwrap();
    let (inserts_output, proof) = tree.extend_with_proof(&inserts, &missing_reads).unwrap();
    let proven_tree_view = proof
        .verify(&Blake2Hasher, 64, None, &inserts, &missing_reads)
        .unwrap();
    assert_eq!(proven_tree_view.root_hash, inserts_output.root_hash);
    assert_eq!(proven_tree_view.read_entries.len(), missing_reads.len());
    for read_key in &missing_reads {
        assert_eq!(proven_tree_view.read_entries[read_key], None);
    }

    // Test a proof with only updates.
    let updates: Vec<_> = inserts
        .choose_multiple(&mut rng, updates_count)
        .map(|entry| TreeEntry {
            key: entry.key,
            value: H256::zero(),
        })
        .collect();

    let (output, proof) = tree.extend_with_proof(&updates, &[]).unwrap();
    let updates_tree_hash = output.root_hash;

    assert_eq!(proof.operations.len(), updates.len());
    let mut updated_indices = vec![];
    for op in &proof.operations {
        match *op {
            TreeOperation::Hit { index } => updated_indices.push(index),
            TreeOperation::Miss { .. } => panic!("unexpected operation: {op:?}"),
        }
    }
    updated_indices.sort_unstable();

    assert_eq!(proof.sorted_leaves.len(), updates.len());
    assert_eq!(
        proof.sorted_leaves.keys().copied().collect::<Vec<_>>(),
        updated_indices
    );

    let proven_tree_view = proof
        .verify(&Blake2Hasher, 64, Some(inserts_output), &updates, &[])
        .unwrap();
    assert_eq!(proven_tree_view.root_hash, updates_tree_hash);
}

#[test]
fn extending_tree_with_proof() {
    for insert_count in [10, 20, 50, 100, 1_000] {
        for update_count in HashSet::from([1, 2, insert_count / 4, insert_count / 2]) {
            println!("insert_count={insert_count}, update_count={update_count}");
            test_extending_tree_with_proof(PatchSet::default(), insert_count, update_count);
        }
    }
}

fn test_incrementally_extending_tree_with_proofs(
    db: impl Database,
    update_count: usize,
    read_count: usize,
) {
    const RNG_SEED: u64 = 123;

    let mut tree = MerkleTree::new(db).unwrap();
    let mut current_state = HashMap::new();
    let empty_tree_output = tree.extend(&[]).unwrap();

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..1_000).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();

    for chunk_size in [1, 2, 3, 5, 8, 13, 21, 34, 55, 100] {
        println!("Update in {chunk_size}-sized chunks");

        let mut tree_output = empty_tree_output;
        for (i, chunk) in inserts.chunks(chunk_size).enumerate() {
            let chunk_start_idx = i * chunk_size;
            // Only choose updates from the previously inserted entries.
            let mut updates: Vec<_> = inserts[..chunk_start_idx]
                .choose_multiple(&mut rng, update_count.min(chunk_start_idx))
                .map(|entry| TreeEntry {
                    key: entry.key,
                    value: H256::repeat_byte(0xff),
                })
                .collect();

            let existing_reads = inserts[..chunk_start_idx]
                .choose_multiple(&mut rng, read_count.min(chunk_start_idx))
                .map(|entry| entry.key);
            let missing_reads = (0..read_count).map(|_| H256(rng.gen()));
            let mut all_reads: Vec<_> = missing_reads.chain(existing_reads).collect();
            all_reads.shuffle(&mut rng);

            updates.shuffle(&mut rng);
            let mut entries = chunk.to_vec();
            entries.extend(updates);

            let (new_output, proof) = tree.extend_with_proof(&entries, &all_reads).unwrap();
            let proven_tree_view = proof
                .verify(&Blake2Hasher, 64, Some(tree_output), &entries, &all_reads)
                .unwrap();
            assert_eq!(proven_tree_view.root_hash, new_output.root_hash);

            for read_key in &all_reads {
                assert_eq!(
                    proven_tree_view.read_entries[read_key],
                    current_state.get(read_key).copied()
                );
            }

            current_state.extend(entries.into_iter().map(|entry| (entry.key, entry.value)));
            tree_output = new_output;
        }

        tree.truncate_recent_versions(1).unwrap();
    }
}

#[test]
fn incrementally_extending_tree_with_proofs() {
    for update_count in [0, 1, 5] {
        for read_count in [0, 1, 10] {
            println!("update_count={update_count}, read_count={read_count}");
            test_incrementally_extending_tree_with_proofs(
                PatchSet::default(),
                update_count,
                read_count,
            );
        }
    }
}

fn test_read_proofs(db: impl Database) {
    const RNG_SEED: u64 = 111;

    let mut tree = MerkleTree::new(db).unwrap();
    let empty_tree_output = tree.extend(&[]).unwrap();

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..1_000).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();
    let mut inserted_keys: Vec<_> = inserts.iter().map(|entry| entry.key).collect();
    inserted_keys.shuffle(&mut rng);

    let new_tree_output = tree.extend(&inserts).unwrap();

    // Create and check a proof at version 0 (i.e., before inserting entries).
    let proof = tree.prove(0, &inserted_keys).unwrap();
    let proven_tree_view = proof
        .verify_reads(&Blake2Hasher, 64, empty_tree_output, &inserted_keys)
        .unwrap();
    assert_eq!(proven_tree_view.root_hash, empty_tree_output.root_hash);
    assert_eq!(proven_tree_view.read_entries.len(), inserted_keys.len());
    for key in &inserted_keys {
        assert_eq!(proven_tree_view.read_entries[key], None);
    }

    // Create a proof for all inserted keys.
    let proof = tree.prove(1, &inserted_keys).unwrap();
    let proven_tree_view = proof
        .verify_reads(&Blake2Hasher, 64, new_tree_output, &inserted_keys)
        .unwrap();
    assert_eq!(proven_tree_view.root_hash, new_tree_output.root_hash);
    assert_eq!(proven_tree_view.read_entries.len(), inserted_keys.len());
    for key in &inserted_keys {
        assert!(proven_tree_view.read_entries[key].is_some());
    }

    // Create proof for key chunks and also mix some missing keys.
    for chunk_size in [1, 2, 3, 5, 8, 13] {
        println!("Using chunk size {chunk_size}");
        for proven_keys in inserted_keys.chunks_exact(chunk_size) {
            let mut proven_keys = proven_keys.to_vec();
            proven_keys.extend((0..chunk_size).map(|_| H256(rng.gen())));

            let proof = tree.prove(1, &proven_keys).unwrap();
            let proven_tree_view = proof
                .verify_reads(&Blake2Hasher, 64, new_tree_output, &proven_keys)
                .unwrap();
            assert_eq!(proven_tree_view.root_hash, new_tree_output.root_hash);
            assert_eq!(proven_tree_view.read_entries.len(), proven_keys.len());
            for (i, key) in proven_keys.iter().enumerate() {
                assert_eq!(proven_tree_view.read_entries[key].is_some(), i < chunk_size);
            }
        }
    }
}

fn test_using_patched_database(db: impl Database) {
    const RNG_SEED: u64 = 321;
    const FLUSH_PROBABILITY: f64 = 0.4;

    let mut tree = MerkleTree::new(Patched::new(db)).unwrap();
    let empty_tree_output = tree.extend(&[]).unwrap();

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let nodes = (0..1_000).map(|_| TreeEntry {
        key: H256(rng.gen()),
        value: H256(rng.gen()),
    });
    let inserts: Vec<_> = nodes.collect();

    for chunk_size in [10, 42, 101, 333, 1_000] {
        println!("Using chunk_size={chunk_size}");

        let mut tree_output = empty_tree_output;
        for chunk in inserts.chunks(chunk_size) {
            let (new_output, proof) = tree.extend_with_proof(chunk, &[]).unwrap();
            proof
                .verify(&Blake2Hasher, 64, Some(tree_output), chunk, &[])
                .unwrap();
            tree_output = new_output;

            let latest_version = tree.latest_version().unwrap().expect("no versions");
            for version in latest_version.saturating_sub(5)..=latest_version {
                println!(
                    "verifying consistency for version={version}, latest_version={latest_version}"
                );
                tree.verify_consistency(version).unwrap();
            }

            if rng.gen_bool(FLUSH_PROBABILITY) {
                tree.db.flush().unwrap();
            }
        }

        tree.truncate_recent_versions(1).unwrap();
    }
}

#[test]
fn using_patched_database() {
    test_using_patched_database(PatchSet::default());
}

#[test]
fn read_proofs() {
    test_read_proofs(PatchSet::default());
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

    #[test]
    fn extending_tree_with_proof() {
        let temp_dir = TempDir::new().unwrap();
        for insert_count in [10, 20, 50, 100, 1_000] {
            for update_count in HashSet::from([1, 2, insert_count / 4, insert_count / 2]) {
                println!("insert_count={insert_count}, update_count={update_count}");
                let db_path = temp_dir
                    .path()
                    .join(format!("{insert_count}-{update_count}"));
                let db = RocksDBWrapper::new(&db_path).unwrap();
                test_extending_tree_with_proof(db, insert_count, update_count);
            }
        }
    }

    #[test]
    fn incrementally_extending_tree_with_proofs() {
        let temp_dir = TempDir::new().unwrap();
        for update_count in [0, 1, 5] {
            for read_count in [0, 1, 10] {
                println!("update_count={update_count}, read_count={read_count}");
                let db_path = temp_dir.path().join(format!("{update_count}-{read_count}"));
                let db = RocksDBWrapper::new(&db_path).unwrap();
                test_incrementally_extending_tree_with_proofs(db, update_count, 0);
            }
        }
    }

    #[test]
    fn read_proofs() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        test_read_proofs(db);
    }

    #[test]
    fn using_patched_database() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        test_using_patched_database(db);
    }
}

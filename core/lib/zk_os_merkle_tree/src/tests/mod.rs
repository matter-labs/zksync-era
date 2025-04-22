//! Tests for the public `MerkleTree` interface.

use std::collections::{BTreeMap, HashMap, HashSet};

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use super::*;
use crate::{
    hasher::TreeOperation,
    storage::{PatchSet, Patched},
    types::{Leaf, Node, NodeKey, TreeTags},
};

mod consistency;
mod prop;

#[test]
fn empty_tree_hash_is_correct() {
    let expected_root_hash: H256 =
        "0x90a83ead2ba2194fbbb0f7cd2a017e36cfb4891513546d943a7282c2844d4b6b"
            .parse()
            .unwrap();
    assert_eq!(
        MerkleTree::<PatchSet>::empty_tree_hash(),
        expected_root_hash
    );
}

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

#[test]
fn key_ordering() {
    let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
    let entries = [
        TreeEntry {
            key: H256::from_low_u64_be(0xc0ffeefe),
            value: H256::repeat_byte(0x10),
        },
        TreeEntry {
            key: H256::from_low_u64_be(0xdeadbeef),
            value: H256::repeat_byte(0x20),
        },
    ];
    let output = tree.extend(&entries).unwrap();

    let expected_root_hash: H256 =
        "0xc90465eddad7cc858a2fbf61013d7051c143887a887e5a7a19344ac32151b207"
            .parse()
            .unwrap();
    assert_eq!(output.root_hash, expected_root_hash);

    let first_key = NodeKey {
        version: 0,
        nibble_count: leaf_nibbles::<DefaultTreeParams>(),
        index_on_level: 2,
    };
    let second_key = NodeKey {
        index_on_level: 3,
        ..first_key
    };
    let leaves = tree.db.try_nodes(&[first_key, second_key]).unwrap();
    let [Node::Leaf(first_leaf), Node::Leaf(second_leaf)] = leaves.as_slice() else {
        panic!("unexpected node: {leaves:?}");
    };
    assert_eq!(first_leaf.key, entries[0].key);
    assert_eq!(second_leaf.key, entries[1].key);
    assert_eq!(first_leaf.next_index, 3);
    assert_eq!(second_leaf.next_index, 1);
}

fn naive_hash_tree(entries: &[TreeEntry]) -> H256 {
    let mut indices = BTreeMap::from([(H256::zero(), 0_u64), (H256::repeat_byte(0xff), 1)]);
    indices.extend(
        entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.key, i as u64 + 2)),
    );
    let next_indices: Vec<_> = indices.values().skip(1).copied().chain([1]).collect();
    let next_indices: HashMap<_, _> = indices.into_keys().zip(next_indices).collect();

    let leaves = [&TreeEntry::MIN_GUARD, &TreeEntry::MAX_GUARD]
        .into_iter()
        .chain(entries)
        .map(|entry| Leaf {
            key: entry.key,
            value: entry.value,
            next_index: next_indices[&entry.key],
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

#[test]
fn extending_tree_with_reference_indices() {
    const RNG_SEED: u64 = 42;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let mut tree = MerkleTree::new(PatchSet::default()).unwrap();

    let nodes = (0_u64..100).map(|i| {
        (
            i + 2,
            TreeEntry {
                key: H256(rng.gen()),
                value: H256(rng.gen()),
            },
        )
    });
    let inserts: Vec<_> = nodes.collect();
    tree.extend_with_reference(&inserts).unwrap();

    let mut updates = inserts;
    for update in &mut updates {
        update.1.value = H256(rng.gen());
    }
    updates.shuffle(&mut rng);

    tree.extend_with_reference(&updates).unwrap();
    assert_eq!(tree.latest_version().unwrap(), Some(1));
    let latest_info = tree.root_info(1).unwrap().expect("no info");

    // Check incorrect index handling
    let err = tree
        .extend_with_reference(&[(0, TreeEntry::MAX_GUARD)])
        .unwrap_err();
    let err = format!("{err:#}");
    assert!(err.contains("Unexpected index"), "{err}");
    assert_eq!(tree.latest_version().unwrap(), Some(1));
    let new_latest_info = tree.root_info(1).unwrap().expect("no info");
    assert_eq!(new_latest_info, latest_info);

    let err = tree
        .extend_with_reference(&[(
            0,
            TreeEntry {
                key: H256(rng.gen()),
                value: H256::zero(),
            },
        )])
        .unwrap_err();
    let err = format!("{err:#}");
    assert!(err.contains("Unexpected index"), "{err}");
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
    use serde::{Deserialize, Serialize};
    use serde_with::{hex::Hex, serde_as};
    use tempfile::TempDir;
    use zksync_storage::RocksDB;

    use super::*;

    type KeyValuePair = (Box<[u8]>, Box<[u8]>);

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct DatabaseSnapshot {
        #[serde_as(as = "BTreeMap<Hex, Hex>")]
        key_indices: Vec<KeyValuePair>,
        #[serde_as(as = "BTreeMap<Hex, Hex>")]
        tree: Vec<KeyValuePair>,
    }

    impl DatabaseSnapshot {
        fn new(raw_db: &RocksDB<MerkleTreeColumnFamily>) -> Self {
            let cf = MerkleTreeColumnFamily::KeyIndices;
            let key_indices: Vec<_> = raw_db.prefix_iterator_cf(cf, &[]).collect();
            assert!(!key_indices.is_empty());
            let cf = MerkleTreeColumnFamily::Tree;
            let tree: Vec<_> = raw_db.prefix_iterator_cf(cf, &[]).collect();
            assert!(!tree.is_empty());
            Self { key_indices, tree }
        }
    }

    #[test]
    fn snapshot_for_empty_tree() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        let mut tree = MerkleTree::new(db).unwrap();
        tree.extend(&[]).unwrap();

        let raw_db = tree.db.into_inner();
        insta::assert_yaml_snapshot!("empty-snapshot", DatabaseSnapshot::new(&raw_db));
    }

    #[test]
    fn snapshot_for_incremental_tree() {
        const RNG_SEED: u64 = 123_321;

        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        let mut rng = StdRng::seed_from_u64(RNG_SEED);
        let mut tree = MerkleTree::new(db).unwrap();
        let all_entries: Vec<_> = (0..50)
            .map(|_| TreeEntry {
                key: H256(rng.gen()),
                value: H256(rng.gen()),
            })
            .collect();
        let mut existing_entry_count = 0;
        while existing_entry_count < all_entries.len() {
            let new_entry_count = rng.gen_range(0..=(all_entries.len() - existing_entry_count));
            let mut new_entries = all_entries
                [existing_entry_count..(existing_entry_count + new_entry_count)]
                .to_vec();

            let updated_count = rng.gen_range(0..=existing_entry_count);
            let updated_entries = all_entries[..existing_entry_count]
                .choose_multiple(&mut rng, updated_count)
                .map(|entry| TreeEntry {
                    value: H256(rng.gen()),
                    ..*entry
                });
            new_entries.extend(updated_entries);
            new_entries.shuffle(&mut rng);

            tree.extend(&new_entries).unwrap();
            existing_entry_count += new_entry_count;
        }

        let raw_db = tree.db.into_inner();
        insta::assert_yaml_snapshot!("incremental-snapshot", DatabaseSnapshot::new(&raw_db));
    }

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

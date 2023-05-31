use assert_matches::assert_matches;

use super::*;
use crate::hasher::{HasherWithStats, MerklePath};
use zksync_types::{H256, U256};

const FIRST_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0000_0000]);
const SECOND_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0100_0000]);
const THIRD_KEY: Key = U256([0, 0, 0, 0x_dead_d00d_1234_5678]);

#[test]
fn inserting_entries_in_empty_database() {
    let db = PatchSet::default();
    let mut updater = TreeUpdater::new(0, Root::Empty);
    assert_eq!(updater.patch_set.version(), 0);
    assert!(updater.root_node_mut().is_none());

    let sorted_keys = SortedKeys::new([FIRST_KEY, SECOND_KEY, THIRD_KEY].into_iter());
    let parent_nibbles = updater.load_ancestors(&sorted_keys, &db);
    assert_eq!(parent_nibbles, [Nibbles::EMPTY; 3]);

    updater.insert(FIRST_KEY, H256([1; 32]), &Nibbles::EMPTY, || 1);

    let root_node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Leaf(root_leaf) = root_node else {
        panic!("Unexpected root node: {root_node:?}");
    };
    assert_eq!(root_leaf.full_key, FIRST_KEY);
    assert_eq!(root_leaf.value_hash, H256([1; 32]));

    updater.insert(SECOND_KEY, H256([2; 32]), &Nibbles::EMPTY, || 2);
    assert_storage_with_2_keys(&updater);

    updater.insert(THIRD_KEY, H256([3; 32]), &Nibbles::EMPTY, || 3);
    assert_storage_with_3_keys(&updater);
}

fn assert_storage_with_2_keys(updater: &TreeUpdater) {
    // Check the internal nodes with a single child that should be created at keys
    // '', 'd', 'de', ..., 'deadbeef'.
    let internal_node_nibbles = (0..8).map(|i| {
        let nibbles = Nibbles::new(&FIRST_KEY, i);
        let next_nibble = Nibbles::nibble(&FIRST_KEY, i);
        (nibbles, next_nibble)
    });
    for (nibbles, next_nibble) in internal_node_nibbles {
        let node = updater.patch_set.get(&nibbles).unwrap();
        let Node::Internal(node) = node else {
            panic!("Unexpected node at {nibbles}: {node:?}");
        };
        assert_eq!(node.child_count(), 1);
        let child_ref = node.child_ref(next_nibble).unwrap();
        assert_eq!(child_ref.version, 0);
        assert!(!child_ref.is_leaf);
    }

    // Check the final internal node with 2 leaf children at 'deadbeef0'.
    let nibbles = Nibbles::new(&FIRST_KEY, 9);
    let node = updater.patch_set.get(&nibbles).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected node at {nibbles}: {node:?}");
    };
    assert_eq!(node.child_count(), 2);
    for next_nibble in [0, 1] {
        let child_ref = node.child_ref(next_nibble).unwrap();
        assert_eq!(child_ref.version, 0);
        assert!(child_ref.is_leaf);
    }

    // Finally, check the leaves.
    let first_leaf_nibbles = Nibbles::new(&FIRST_KEY, 10);
    let node = updater.patch_set.get(&first_leaf_nibbles).unwrap();
    let Node::Leaf(leaf) = node else {
        panic!("Unexpected node at {first_leaf_nibbles}: {node:?}");
    };
    assert_eq!(leaf.full_key, FIRST_KEY);
    assert_eq!(leaf.value_hash, H256([1; 32]));

    let second_leaf_nibbles = Nibbles::new(&SECOND_KEY, 10);
    assert_ne!(second_leaf_nibbles, first_leaf_nibbles);
    let node = updater.patch_set.get(&second_leaf_nibbles).unwrap();
    let Node::Leaf(leaf) = node else {
        panic!("Unexpected node at {second_leaf_nibbles}: {node:?}");
    };
    assert_eq!(leaf.full_key, SECOND_KEY);
    assert_eq!(leaf.value_hash, H256([2; 32]));
}

fn assert_storage_with_3_keys(updater: &TreeUpdater) {
    // The 'dead' internal node should now contain 'b' and 'd' children.
    let nibbles = Nibbles::new(&FIRST_KEY, 4);
    let node = updater.patch_set.get(&nibbles).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected node at {nibbles}: {node:?}");
    };
    assert_eq!(node.child_count(), 2);

    let child_ref = node.child_ref(0xb).unwrap();
    assert!(!child_ref.is_leaf);
    let child_ref = node.child_ref(0xd).unwrap();
    assert!(child_ref.is_leaf);

    let third_leaf_nibbles = Nibbles::new(&THIRD_KEY, 5);
    let node = updater.patch_set.get(&third_leaf_nibbles).unwrap();
    let Node::Leaf(leaf) = node else {
        panic!("Unexpected node at {third_leaf_nibbles}: {node:?}");
    };
    assert_eq!(leaf.full_key, THIRD_KEY);
    assert_eq!(leaf.value_hash, H256([3; 32]));
}

#[test]
fn changing_child_ref_type() {
    let mut updater = TreeUpdater::new(0, Root::Empty);
    updater.insert(FIRST_KEY, H256([1; 32]), &Nibbles::EMPTY, || 1);
    let e_key = U256([0, 0, 0, 0x_e000_0000_0000_0000]);
    updater.insert(e_key, H256([2; 32]), &Nibbles::EMPTY, || 2);

    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected root node: {node:?}");
    };
    assert!(node.child_ref(0xd).unwrap().is_leaf);
    assert!(node.child_ref(0xe).unwrap().is_leaf);

    updater.insert(SECOND_KEY, H256([3; 32]), &Nibbles::EMPTY, || 3);

    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected root node: {node:?}");
    };
    assert!(!node.child_ref(0xd).unwrap().is_leaf);
    assert!(node.child_ref(0xe).unwrap().is_leaf);
}

#[test]
fn inserting_node_in_non_empty_database() {
    const E_KEY: U256 = U256([0, 0, 0, 0x_e000_0000_0000_0000]);

    let mut db = PatchSet::default();
    let storage = Storage::new(&db, 0);
    let kvs = vec![(FIRST_KEY, H256([1; 32])), (SECOND_KEY, H256([2; 32]))];
    let (_, patch) = storage.extend(&(), kvs);
    db.apply_patch(patch);

    let mut count = 2;
    let mut leaf_index_fn = || increment_counter(&mut count);
    let mut updater = TreeUpdater::new(1, db.root(0).unwrap());
    let sorted_keys = SortedKeys::new([THIRD_KEY, E_KEY, SECOND_KEY].into_iter());
    let parent_nibbles = updater.load_ancestors(&sorted_keys, &db);
    assert_eq!(updater.metrics.db_reads, 10);
    assert_eq!(
        parent_nibbles,
        [
            Nibbles::new(&THIRD_KEY, 4), // dead
            Nibbles::EMPTY,
            Nibbles::new(&SECOND_KEY, 10), // deadbeef01
        ]
    );

    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("unexpected root node: {node:?}");
    };
    // Check that child refs for the loaded children were updated.
    assert_eq!(node.child_ref(0xd).unwrap().version, 1);

    let (op, _) = updater.insert(
        THIRD_KEY,
        H256([3; 32]),
        &parent_nibbles[0],
        &mut leaf_index_fn,
    );
    assert_eq!(op, TreeLogEntry::insert(3));
    let (op, _) = updater.insert(E_KEY, H256::zero(), &parent_nibbles[1], &mut leaf_index_fn);
    assert_eq!(op, TreeLogEntry::insert(4));
    let (op, _) = updater.insert(
        SECOND_KEY,
        H256([2; 32]),
        &parent_nibbles[2],
        &mut leaf_index_fn,
    );
    assert_matches!(op, TreeLogEntry::Updated { leaf_index: 2, .. });
    assert_eq!(updater.metrics.new_internal_nodes, 0);
    assert_eq!(updater.metrics.new_leaves, 2);

    // Check that all necessary child refs have updated versions.
    let node = &updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("unexpected root node: {node:?}");
    };
    assert_eq!(node.child_ref(0xe).unwrap().version, 1);

    assert_storage_with_3_keys(&updater);
}

#[test]
fn inserting_node_in_non_empty_database_with_moved_key() {
    let mut db = PatchSet::default();
    let storage = Storage::new(&db, 0);
    let kvs = vec![(FIRST_KEY, H256([1; 32])), (THIRD_KEY, H256([3; 32]))];
    let (_, patch) = storage.extend(&(), kvs);
    db.apply_patch(patch);

    let mut updater = TreeUpdater::new(1, db.root(0).unwrap());
    let sorted_keys = SortedKeys::new([SECOND_KEY].into_iter());
    let parent_nibbles = updater.load_ancestors(&sorted_keys, &db);
    assert_eq!(
        parent_nibbles,
        [Nibbles::new(&SECOND_KEY, 5)] // `deadb`, a leaf node
    );
    assert_matches!(
        updater.patch_set.get(&parent_nibbles[0]),
        Some(Node::Leaf(_))
    );

    let (op, _) = updater.insert(SECOND_KEY, H256([2; 32]), &parent_nibbles[0], || 3);
    assert_eq!(op, TreeLogEntry::insert(3));
    assert_matches!(
        updater.patch_set.get(&parent_nibbles[0]),
        Some(Node::Internal(_))
    );
    assert_eq!(updater.metrics.new_leaves, 1);
    assert_eq!(updater.metrics.moved_leaves, 1);
}

#[test]
fn proving_keys_existence_and_absence() {
    let mut updater = TreeUpdater::new(0, Root::Empty);
    updater.patch_set.ensure_internal_root_node(); // Necessary for proofs to work.
    updater.insert(FIRST_KEY, H256([1; 32]), &Nibbles::EMPTY, || 1);

    let mut hasher = (&() as &dyn HashTree).into();
    let (op, merkle_path) = updater.prove(&mut hasher, FIRST_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::Read { .. });
    let merkle_path = finalize_merkle_path(merkle_path, &mut hasher);
    assert!(merkle_path.is_empty()); // all adjacent hashes correspond to empty subtrees

    let (op, merkle_path) = updater.prove(&mut hasher, SECOND_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::ReadMissingKey);
    let merkle_path = finalize_merkle_path(merkle_path, &mut hasher);
    assert_eq!(merkle_path.len(), 40);

    updater.insert(THIRD_KEY, H256([3; 32]), &Nibbles::EMPTY, || 2);
    let (op, merkle_path) = updater.prove(&mut hasher, FIRST_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::Read { .. });
    let merkle_path = finalize_merkle_path(merkle_path, &mut hasher);
    assert_eq!(merkle_path.len(), 18); // keys diverge at 18th bit

    let (op, merkle_path) = updater.prove(&mut hasher, SECOND_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::ReadMissingKey);
    let merkle_path = finalize_merkle_path(merkle_path, &mut hasher);
    assert_eq!(merkle_path.len(), 40);

    assert_eq!(updater.metrics.key_reads, 2);
    assert_eq!(updater.metrics.missing_key_reads, 2);
}

// Emulate Merkle path finalization.
fn finalize_merkle_path(mut path: MerklePath, hasher: &mut HasherWithStats<'_>) -> Vec<ValueHash> {
    for _ in 0..4 {
        path.push(hasher, None);
    }
    path.into_inner()
}

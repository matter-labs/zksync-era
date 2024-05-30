use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use rand::{
    rngs::StdRng,
    seq::{IteratorRandom, SliceRandom},
    Rng, SeedableRng,
};
use test_casing::test_casing;
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_types::{H256, U256};

use super::*;
use crate::{
    hasher::{HasherWithStats, MerklePath},
    types::{NodeKey, TreeInstruction, KEY_SIZE},
};

pub(super) const FIRST_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0000_0000]);
const SECOND_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0100_0000]);
const THIRD_KEY: Key = U256([0, 0, 0, 0x_dead_d00d_1234_5678]);
const E_KEY: U256 = U256([0, 0, 0, 0x_e000_0000_0000_0000]);

pub(super) fn generate_nodes(version: u64, nibble_counts: &[usize]) -> HashMap<NodeKey, Node> {
    let nodes = nibble_counts.iter().map(|&count| {
        assert_ne!(count, 0);
        let key = Nibbles::new(&FIRST_KEY, count).with_version(version);
        let node = LeafNode::new(TreeEntry::new(FIRST_KEY, count as u64, H256::zero()));
        (key, node.into())
    });
    nodes.collect()
}

pub(super) fn create_patch(
    latest_version: u64,
    root: Root,
    nodes: HashMap<NodeKey, Node>,
) -> PatchSet {
    let manifest = Manifest::new(latest_version + 1, &());
    PatchSet::new(
        manifest,
        latest_version,
        root,
        nodes,
        vec![],
        Operation::Insert,
    )
}

#[test]
fn inserting_entries_in_empty_database() {
    let db = PatchSet::default();
    let mut updater = TreeUpdater::new(0, Root::Empty);
    assert_eq!(updater.patch_set.root_version(), 0);
    assert!(updater.patch_set.get(&Nibbles::EMPTY).is_none());

    let sorted_keys = SortedKeys::new([FIRST_KEY, SECOND_KEY, THIRD_KEY].into_iter());
    let parent_nibbles = updater.load_ancestors(&sorted_keys, &db);
    assert_eq!(parent_nibbles, [Nibbles::EMPTY; 3]);

    updater.insert(TreeEntry::new(FIRST_KEY, 1, H256([1; 32])), &Nibbles::EMPTY);

    let root_node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Leaf(root_leaf) = root_node else {
        panic!("Unexpected root node: {root_node:?}");
    };
    assert_eq!(root_leaf.full_key, FIRST_KEY);
    assert_eq!(root_leaf.value_hash, H256([1; 32]));

    updater.insert(
        TreeEntry::new(SECOND_KEY, 2, H256([2; 32])),
        &Nibbles::EMPTY,
    );
    assert_storage_with_2_keys(&updater);

    updater.insert(TreeEntry::new(THIRD_KEY, 3, H256([3; 32])), &Nibbles::EMPTY);
    assert_storage_with_3_keys(&updater);
}

fn assert_storage_with_2_keys(updater: &TreeUpdater) {
    // Check the internal nodes with a single child that should be created at keys
    // `'', 'd', 'de', ..., 'deadbeef'`.
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

    // Check the final internal node with 2 leaf children at `deadbeef0`.
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
    updater.insert(TreeEntry::new(FIRST_KEY, 1, H256([1; 32])), &Nibbles::EMPTY);
    let e_key = U256([0, 0, 0, 0x_e000_0000_0000_0000]);
    updater.insert(TreeEntry::new(e_key, 2, H256([2; 32])), &Nibbles::EMPTY);

    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected root node: {node:?}");
    };
    assert!(node.child_ref(0xd).unwrap().is_leaf);
    assert!(node.child_ref(0xe).unwrap().is_leaf);

    updater.insert(
        TreeEntry::new(SECOND_KEY, 3, H256([3; 32])),
        &Nibbles::EMPTY,
    );

    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("Unexpected root node: {node:?}");
    };
    assert!(!node.child_ref(0xd).unwrap().is_leaf);
    assert!(node.child_ref(0xe).unwrap().is_leaf);
}

#[test]
fn inserting_node_in_non_empty_database() {
    let mut db = PatchSet::default();
    let storage = Storage::new(&db, &(), 0, true);
    let kvs = vec![
        TreeEntry::new(FIRST_KEY, 1, H256([1; 32])),
        TreeEntry::new(SECOND_KEY, 2, H256([2; 32])),
    ];
    let (_, patch) = storage.extend(kvs);
    db.apply_patch(patch).unwrap();

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

    let (op, _) = updater.insert(
        TreeEntry::new(THIRD_KEY, 3, H256([3; 32])),
        &parent_nibbles[0],
    );
    assert_eq!(op, TreeLogEntry::Inserted);
    let (op, _) = updater.insert(TreeEntry::new(E_KEY, 4, H256::zero()), &parent_nibbles[1]);
    assert_eq!(op, TreeLogEntry::Inserted);
    let (op, _) = updater.insert(
        TreeEntry::new(SECOND_KEY, 2, H256([2; 32])),
        &parent_nibbles[2],
    );
    assert_matches!(op, TreeLogEntry::Updated { leaf_index: 2, .. });
    assert_eq!(updater.metrics.new_internal_nodes, 0);
    assert_eq!(updater.metrics.new_leaves, 2);

    // Check that all necessary child refs have updated versions.
    let node = updater.patch_set.get(&Nibbles::EMPTY).unwrap();
    let Node::Internal(node) = node else {
        panic!("unexpected root node: {node:?}");
    };
    // Check that child refs for the loaded children were updated.
    assert_eq!(node.child_ref(0xd).unwrap().version, 1);
    assert_eq!(node.child_ref(0xe).unwrap().version, 1);

    assert_storage_with_3_keys(&updater);
}

#[test]
fn inserting_node_in_non_empty_database_with_moved_key() {
    let mut db = PatchSet::default();
    let storage = Storage::new(&db, &(), 0, true);
    let kvs = vec![
        TreeEntry::new(FIRST_KEY, 1, H256([1; 32])),
        TreeEntry::new(THIRD_KEY, 2, H256([3; 32])),
    ];
    let (_, patch) = storage.extend(kvs);
    db.apply_patch(patch).unwrap();

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

    let (op, _) = updater.insert(
        TreeEntry::new(SECOND_KEY, 3, H256([2; 32])),
        &parent_nibbles[0],
    );
    assert_eq!(op, TreeLogEntry::Inserted);
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
    updater.insert(TreeEntry::new(FIRST_KEY, 1, H256([1; 32])), &Nibbles::EMPTY);

    let mut hasher = HasherWithStats::new(&());
    let (op, merkle_path) = updater.prove(&mut hasher, FIRST_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::Read { .. });
    let merkle_path = finalize_merkle_path(merkle_path, &hasher);
    assert!(merkle_path.is_empty()); // all adjacent hashes correspond to empty subtrees

    let (op, merkle_path) = updater.prove(&mut hasher, SECOND_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::ReadMissingKey);
    let merkle_path = finalize_merkle_path(merkle_path, &hasher);
    assert_eq!(merkle_path.len(), 40);

    updater.insert(TreeEntry::new(THIRD_KEY, 2, H256([3; 32])), &Nibbles::EMPTY);
    let (op, merkle_path) = updater.prove(&mut hasher, FIRST_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::Read { .. });
    let merkle_path = finalize_merkle_path(merkle_path, &hasher);
    assert_eq!(merkle_path.len(), 18); // keys diverge at 18th bit

    let (op, merkle_path) = updater.prove(&mut hasher, SECOND_KEY, &Nibbles::EMPTY);
    assert_matches!(op, TreeLogEntry::ReadMissingKey);
    let merkle_path = finalize_merkle_path(merkle_path, &hasher);
    assert_eq!(merkle_path.len(), 40);

    assert_eq!(updater.metrics.key_reads, 2);
    assert_eq!(updater.metrics.missing_key_reads, 2);
}

// Emulate Merkle path finalization.
fn finalize_merkle_path(mut path: MerklePath, hasher: &HasherWithStats<'_>) -> Vec<ValueHash> {
    for _ in 0..4 {
        path.push(hasher, None);
    }
    path.into_inner()
}

#[test]
fn reading_keys_does_not_change_child_version() {
    let mut db = PatchSet::default();
    let storage = Storage::new(&db, &(), 0, true);
    let kvs = vec![
        TreeEntry::new(FIRST_KEY, 1, H256([0; 32])),
        TreeEntry::new(SECOND_KEY, 2, H256([1; 32])),
    ];
    let (_, patch) = storage.extend(kvs);
    db.apply_patch(patch).unwrap();

    let storage = Storage::new(&db, &(), 1, true);
    let instructions = vec![
        TreeInstruction::Read(FIRST_KEY),
        TreeInstruction::Write(TreeEntry::new(E_KEY, 3, H256([2; 32]))),
    ];

    let (_, patch) = storage.extend_with_proofs(instructions);
    let Some(Root::Filled {
        leaf_count,
        node: Node::Internal(node),
    }) = &patch.patches_by_version[&1].root
    else {
        panic!("unexpected root");
    };
    assert_eq!(u64::from(*leaf_count), 3);
    assert_eq!(node.child_ref(0xd).unwrap().version, 0);
    assert_eq!(node.child_ref(0xe).unwrap().version, 1);
}

#[test]
fn read_ops_are_not_reflected_in_patch() {
    let mut db = PatchSet::default();
    let storage = Storage::new(&db, &(), 0, true);
    let kvs = vec![
        TreeEntry::new(FIRST_KEY, 1, H256([0; 32])),
        TreeEntry::new(SECOND_KEY, 2, H256([1; 32])),
    ];
    let (_, patch) = storage.extend(kvs);
    db.apply_patch(patch).unwrap();

    let storage = Storage::new(&db, &(), 1, true);
    let instructions = vec![TreeInstruction::Read(FIRST_KEY)];
    let (_, patch) = storage.extend_with_proofs(instructions);
    assert!(patch.patches_by_version[&1].nodes.is_empty());
}

// This maps small indices to keys that differ in the starting nibbles.
fn big_endian_key(index: u64) -> U256 {
    U256([0, 0, 0, index.swap_bytes()])
}

#[test_casing(3, [10, 20, 50])]
fn read_instructions_do_not_lead_to_copied_nodes(writes_per_block: u64) {
    const RNG_SEED: u64 = 12;

    // Write some keys into the database.
    let mut key_count = writes_per_block;
    let mut database = PatchSet::default();
    let storage = Storage::new(&database, &(), 0, true);
    let kvs = (0..key_count)
        .map(|i| TreeEntry::new(big_endian_key(i), i + 1, H256::zero()))
        .collect();
    let (_, patch) = storage.extend(kvs);
    database.apply_patch(patch).unwrap();

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    for _ in 0..100 {
        // Select some existing keys to read. Keys may be repeated, this is fine for our purpose.
        let reads = (0..writes_per_block).map(|_| {
            let key = big_endian_key(rng.gen_range(0..key_count));
            TreeInstruction::Read(key)
        });
        let writes = (key_count..key_count + writes_per_block).map(|i| {
            TreeInstruction::Write(TreeEntry::new(big_endian_key(i), i + 1, H256::zero()))
        });

        let mut instructions: Vec<_> = reads.chain(writes).collect();
        instructions.shuffle(&mut rng);
        key_count += writes_per_block;

        let storage = Storage::new(&database, &(), 1, true);
        let (_, patch) = storage.extend_with_proofs(instructions);
        assert_no_copied_nodes(&database, &patch);
        database.apply_patch(patch).unwrap();
    }
}

fn assert_no_copied_nodes(database: &PatchSet, patch: &PatchSet) {
    assert_eq!(patch.patches_by_version.len(), 1);

    let (&version, patch) = patch.patches_by_version.iter().next().unwrap();
    for (key, node) in &patch.nodes {
        let prev_node = (0..version).rev().find_map(|v| {
            let prev_key = key.nibbles.with_version(v);
            database.patches_by_version[&v].nodes.get(&prev_key)
        });
        if let Some(prev_node) = prev_node {
            assert_ne!(node, prev_node, "node at {key:?} is copied");
        }
    }
}

#[test_casing(12, test_casing::Product(([1, 3, 5, 10, 20, 50], [false, true])))]
fn replaced_keys_are_correctly_tracked(writes_per_block: usize, with_proofs: bool) {
    const RNG_SEED: u64 = 12;

    // Write some keys into the database.
    let mut database = PatchSet::default();
    let storage = Storage::new(&database, &(), 0, true);
    let kvs = (0..100)
        .map(|i| TreeEntry::new(big_endian_key(i), i + 1, H256::zero()))
        .collect();
    let (_, patch) = storage.extend(kvs);

    assert!(patch.stale_keys_by_version[&0].is_empty());
    database.apply_patch(patch).unwrap();

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    for new_version in 1..=100 {
        let updates = (0..100)
            .choose_multiple(&mut rng, writes_per_block)
            .into_iter()
            .map(|i| TreeEntry::new(big_endian_key(i), i + 1, H256::zero()));

        let storage = Storage::new(&database, &(), new_version, true);
        let patch = if with_proofs {
            let instructions = updates.map(TreeInstruction::Write);
            storage.extend_with_proofs(instructions.collect()).1
        } else {
            storage.extend(updates.collect()).1
        };
        assert_replaced_keys(&database, &patch);
        database.apply_patch(patch).unwrap();
    }
}

fn assert_replaced_keys(db: &PatchSet, patch: &PatchSet) {
    assert_eq!(patch.patches_by_version.len(), 1);
    let (&version, sub_patch) = patch.patches_by_version.iter().next().unwrap();
    assert_eq!(patch.stale_keys_by_version.len(), 1);
    let replaced_keys = patch.stale_keys_by_version.values().next().unwrap();

    let expected_replaced_keys = sub_patch.nodes.keys().filter_map(|key| {
        (0..key.version).rev().find_map(|v| {
            let prev_key = key.nibbles.with_version(v);
            let contains_key = db
                .patches_by_version
                .get(&prev_key.version)?
                .nodes
                .contains_key(&prev_key);
            contains_key.then_some(prev_key)
        })
    });
    let expected_replaced_keys: HashSet<_> = expected_replaced_keys
        .chain([Nibbles::EMPTY.with_version(version - 1)]) // add the root key
        .collect();

    let replaced_keys: HashSet<_> = replaced_keys.iter().copied().collect();
    assert_eq!(replaced_keys, expected_replaced_keys);
}

#[test]
fn tree_handles_keys_at_terminal_level() {
    let mut db = PatchSet::default();
    let kvs = (0_u64..100)
        .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::zero()))
        .collect();
    let (_, patch) = Storage::new(&db, &(), 0, true).extend(kvs);
    db.apply_patch(patch).unwrap();

    // Overwrite a key and check that we don't panic.
    let new_kvs = vec![TreeEntry::new(
        Key::from(0),
        1,
        ValueHash::from_low_u64_be(1),
    )];
    let (_, patch) = Storage::new(&db, &(), 1, true).extend(new_kvs);

    assert_eq!(
        patch.patches_by_version[&1]
            .root
            .as_ref()
            .unwrap()
            .leaf_count(),
        100
    );
    assert_eq!(patch.patches_by_version[&1].nodes.len(), 2 * KEY_SIZE); // root is counted separately
    for (key, node) in &patch.patches_by_version[&1].nodes {
        let is_terminal = key.nibbles.nibble_count() == 2 * KEY_SIZE;
        assert_eq!(is_terminal, matches!(node, Node::Leaf(_)));
    }
    assert_eq!(patch.stale_keys_by_version[&1].len(), 2 * KEY_SIZE + 1);
}

#[test]
fn recovery_flattens_node_versions() {
    let recovery_version = 100;
    let recovery_entries = (0_u64..10).map(|i| TreeEntry {
        key: Key::from(i) << 252, // the first key nibbles are distinct
        value: ValueHash::zero(),
        leaf_index: i + 1,
    });
    let patch = Storage::new(&PatchSet::default(), &(), recovery_version, false)
        .extend_during_linear_recovery(recovery_entries.collect());
    assert_eq!(patch.patches_by_version.len(), 1);
    let (updated_version, patch) = patch.patches_by_version.into_iter().next().unwrap();
    assert_eq!(updated_version, recovery_version);

    let root = patch.root.unwrap();
    assert_eq!(root.leaf_count(), 10);
    let Root::Filled {
        node: Node::Internal(root_node),
        ..
    } = &root
    else {
        panic!("Unexpected root: {root:?}");
    };
    for nibble in 0..10 {
        assert_eq!(
            root_node.child_ref(nibble).unwrap().version,
            recovery_version
        );
        let expected_key = Nibbles::single(nibble).with_version(recovery_version);
        assert_matches!(patch.nodes[&expected_key], Node::Leaf { .. });
    }
}

#[test_casing(7, [256, 4, 5, 20, 69, 127, 128])]
fn recovery_with_node_hierarchy(chunk_size: usize) {
    let recovery_version = 100;
    let recovery_entries = (0_u64..256).map(|i| TreeEntry {
        key: Key::from(i) << 248, // the first two key nibbles are distinct
        value: ValueHash::zero(),
        leaf_index: i + 1,
    });
    let recovery_entries: Vec<_> = recovery_entries.collect();

    let mut db = PatchSet::default();
    for recovery_chunk in recovery_entries.chunks(chunk_size) {
        let patch = Storage::new(&db, &(), recovery_version, false)
            .extend_during_linear_recovery(recovery_chunk.to_vec());
        db.apply_patch(patch).unwrap();
    }
    assert_eq!(db.updated_version, Some(recovery_version));
    let patch = db.patches_by_version.remove(&recovery_version).unwrap();

    let root = patch.root.unwrap();
    assert_eq!(root.leaf_count(), 256);
    let Root::Filled {
        node: Node::Internal(root_node),
        ..
    } = &root
    else {
        panic!("Unexpected root: {root:?}");
    };

    for nibble in 0..16 {
        let child_ref = root_node.child_ref(nibble).unwrap();
        assert!(!child_ref.is_leaf);
        assert_eq!(child_ref.version, recovery_version);

        let internal_node_key = Nibbles::single(nibble).with_version(recovery_version);
        let node = &patch.nodes[&internal_node_key];
        let Node::Internal(node) = node else {
            panic!("Unexpected upper-level node: {node:?}");
        };
        assert_eq!(node.child_count(), 16);

        for (second_nibble, child_ref) in node.children() {
            let i = nibble * 16 + second_nibble;
            assert!(child_ref.is_leaf);
            assert_eq!(child_ref.version, recovery_version);
            let leaf_key = Nibbles::new(&(Key::from(i) << 248), 2).with_version(recovery_version);
            assert_matches!(patch.nodes[&leaf_key], Node::Leaf { .. });
        }
    }
}

#[test_casing(7, [256, 5, 7, 20, 59, 127, 128])]
fn recovery_with_deep_node_hierarchy(chunk_size: usize) {
    let recovery_version = 1_000;
    let recovery_entries = (0_u64..256).map(|i| TreeEntry {
        key: Key::from(i), // the last two key nibbles are distinct
        value: ValueHash::zero(),
        leaf_index: i + 1,
    });
    let recovery_entries: Vec<_> = recovery_entries.collect();

    let mut db = PatchSet::default();
    for recovery_chunk in recovery_entries.chunks(chunk_size) {
        let patch = Storage::new(&db, &(), recovery_version, false)
            .extend_during_linear_recovery(recovery_chunk.to_vec());
        db.apply_patch(patch).unwrap();
    }
    let mut patch = db.patches_by_version.remove(&recovery_version).unwrap();
    // Manually remove all stale keys from the patch
    assert_eq!(db.stale_keys_by_version.len(), 1);
    for stale_key in &db.stale_keys_by_version[&recovery_version] {
        assert!(
            patch.nodes.remove(stale_key).is_some(),
            "Stale key {stale_key} is missing"
        );
    }

    let root = patch.root.unwrap();
    assert_eq!(root.leaf_count(), 256);
    let Root::Filled {
        node: Node::Internal(root_node),
        ..
    } = &root
    else {
        panic!("Unexpected root: {root:?}");
    };
    assert_eq!(root_node.child_count(), 1);
    let child_ref = root_node.child_ref(0).unwrap();
    assert!(!child_ref.is_leaf);
    assert_eq!(child_ref.version, recovery_version);

    for (node_key, node) in patch.nodes {
        assert_eq!(
            node_key.version, recovery_version,
            "Unexpected version for {node_key}"
        );

        let nibble_count = node_key.nibbles.nibble_count();
        if nibble_count < 64 {
            let Node::Internal(node) = node else {
                panic!("Unexpected node at {node_key}: {node:?}");
            };
            assert_eq!(node.child_count(), if nibble_count < 62 { 1 } else { 16 });
        } else {
            assert_matches!(
                node,
                Node::Leaf(_),
                "Unexpected node at {node_key}: {node:?}"
            );
        }
    }
}

#[test]
fn recovery_workflow_with_multiple_stages() {
    let mut db = PatchSet::default();
    let recovery_version = 100;
    let recovery_entries = (0_u64..100).map(|i| TreeEntry {
        key: Key::from(i),
        value: ValueHash::zero(),
        leaf_index: i,
    });
    let patch = Storage::new(&db, &(), recovery_version, false)
        .extend_during_linear_recovery(recovery_entries.collect());
    assert_eq!(patch.root(recovery_version).unwrap().leaf_count(), 100);
    db.apply_patch(patch).unwrap();

    let more_recovery_entries = (100_u64..200).map(|i| TreeEntry {
        key: Key::from(i),
        value: ValueHash::zero(),
        leaf_index: i,
    });

    let patch = Storage::new(&db, &(), recovery_version, false)
        .extend_during_linear_recovery(more_recovery_entries.collect());
    assert_eq!(patch.root(recovery_version).unwrap().leaf_count(), 200);
    db.apply_patch(patch).unwrap();

    // Check that all entries can be accessed
    let storage = Storage::new(&db, &(), recovery_version + 1, true);
    let instructions = (0_u32..200).map(|i| TreeInstruction::Read(Key::from(i)));
    let (output, _) = storage.extend_with_proofs(instructions.collect());
    assert_eq!(output.leaf_count, 200);
    assert_eq!(output.logs.len(), 200);
    assert!(output
        .logs
        .iter()
        .all(|log| matches!(log.base, TreeLogEntry::Read { .. })));
}

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Linear,
    Random,
}

impl RecoveryKind {
    const ALL: [Self; 2] = [Self::Linear, Self::Random];
}

fn test_recovery_pruning_equivalence(
    kind: RecoveryKind,
    chunk_size: usize,
    recovery_chunk_size: usize,
    hasher: &dyn HashTree,
) {
    const RNG_SEED: u64 = 123;

    println!(
        "Testing recovery–pruning equivalence (chunk size: {chunk_size}, recovery chunk size: \
         {recovery_chunk_size})"
    );

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let entries = (0..100).map(|i| {
        let key = U256([rng.gen(), rng.gen(), rng.gen(), rng.gen()]);
        TreeEntry::new(key, u64::from(i) + 1, ValueHash::repeat_byte(i))
    });
    let entries: Vec<_> = entries.collect();

    // Add `kvs` into the tree in several commits.
    let mut db = PatchSet::default();
    for (version, chunk) in entries.chunks(chunk_size).enumerate() {
        let (_, patch) = Storage::new(&db, hasher, version as u64, true).extend(chunk.to_vec());
        db.apply_patch(patch).unwrap();
    }
    // Unite all remaining nodes to a map and manually remove all stale keys.
    let recovered_version = db.manifest.version_count - 1;
    let mut root = db.root(recovered_version).unwrap();
    let mut all_nodes: HashMap<_, _> = db
        .patches_by_version
        .into_values()
        .flat_map(|sub_patch| sub_patch.nodes)
        .collect();
    for stale_key in db.stale_keys_by_version.values().flatten() {
        all_nodes.remove(stale_key);
    }

    // Generate recovery entries.
    let recovery_entries = all_nodes.values().filter_map(|node| {
        if let Node::Leaf(leaf) = node {
            return Some(TreeEntry::from(*leaf));
        }
        None
    });
    let mut recovery_entries: Vec<_> = recovery_entries.collect();
    assert_eq!(recovery_entries.len(), 100);
    match kind {
        RecoveryKind::Linear => recovery_entries.sort_unstable_by_key(|entry| entry.key),
        RecoveryKind::Random => recovery_entries.shuffle(&mut rng),
    }

    // Recover the tree.
    let mut recovered_db = PatchSet::default();
    for recovery_chunk in recovery_entries.chunks(recovery_chunk_size) {
        let storage = Storage::new(&recovered_db, hasher, recovered_version, false);
        let patch = match kind {
            RecoveryKind::Linear => storage.extend_during_linear_recovery(recovery_chunk.to_vec()),
            RecoveryKind::Random => storage.extend_during_random_recovery(recovery_chunk.to_vec()),
        };
        recovered_db.apply_patch(patch).unwrap();
    }
    let sub_patch = recovered_db
        .patches_by_version
        .remove(&recovered_version)
        .unwrap();
    let recovered_root = sub_patch.root.unwrap();
    let mut all_recovered_nodes = sub_patch.nodes;
    for stale_key in db.stale_keys_by_version.values().flatten() {
        all_recovered_nodes.remove(stale_key);
    }

    // Nodes must be identical for the pruned and recovered trees up to the version.
    if let Root::Filled {
        node: Node::Internal(node),
        ..
    } = &mut root
    {
        for child_ref in node.child_refs_mut() {
            child_ref.version = recovered_version;
        }
    }
    assert_eq!(recovered_root, root);

    let flattened_version_nodes: HashMap<_, _> = all_nodes
        .into_iter()
        .map(|(key, mut node)| {
            if let Node::Internal(node) = &mut node {
                for child_ref in node.child_refs_mut() {
                    child_ref.version = recovered_version;
                }
            }
            (key.nibbles.with_version(recovered_version), node)
        })
        .collect();
    assert_eq!(all_recovered_nodes, flattened_version_nodes);
}

const HASHERS: [&'static dyn HashTree; 2] = [&(), &Blake2Hasher];
const CHUNK_SIZES: [usize; 8] = [3, 5, 7, 11, 21, 42, 99, 100];

#[test_casing(32, test_casing::Product((RecoveryKind::ALL, HASHERS, CHUNK_SIZES)))]
#[test]
fn recovery_pruning_equivalence(
    kind: RecoveryKind,
    hasher: &'static dyn HashTree,
    chunk_size: usize,
) {
    // No chunking during recovery (simple case).
    test_recovery_pruning_equivalence(kind, chunk_size, 100, hasher);
    // Recovery is chunked (more complex case).
    for recovery_chunk_size in [chunk_size, 1, 19, 73] {
        test_recovery_pruning_equivalence(kind, chunk_size, recovery_chunk_size, hasher);
    }
}

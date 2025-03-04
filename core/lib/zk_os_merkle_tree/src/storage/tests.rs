use std::collections::HashSet;

use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

use super::*;
use crate::{DefaultTreeParams, MerkleTree, TreeEntry, TreeParams};

#[test]
fn creating_min_update_for_empty_tree() {
    let update = TreeUpdate::for_empty_tree(&[]).unwrap();
    assert_eq!(update.version, 0);
    assert!(update.updates.is_empty());

    assert_eq!(update.inserts.len(), 2);
    assert_eq!(update.inserts[0], Leaf::MIN_GUARD);
    assert_eq!(update.inserts[1], Leaf::MAX_GUARD);

    assert_eq!(update.sorted_new_leaves.len(), 2);
    assert_eq!(
        update.sorted_new_leaves[&H256::zero()],
        InsertedKeyEntry {
            index: 0,
            inserted_at: 0,
        }
    );
    assert_eq!(
        update.sorted_new_leaves[&H256::repeat_byte(0xff)],
        InsertedKeyEntry {
            index: 1,
            inserted_at: 0,
        }
    );
}

#[test]
fn creating_non_empty_update_for_empty_tree() {
    let update = TreeUpdate::for_empty_tree(&[
        TreeEntry {
            key: H256::repeat_byte(2),
            value: H256::from_low_u64_be(1),
        },
        TreeEntry {
            key: H256::repeat_byte(1),
            value: H256::from_low_u64_be(2),
        },
    ])
    .unwrap();
    assert_eq!(update.version, 0);
    assert!(update.updates.is_empty());

    assert_eq!(update.inserts.len(), 4);
    assert_eq!(
        update.inserts[0],
        Leaf {
            next_index: 3,
            ..Leaf::MIN_GUARD
        }
    );
    assert_eq!(
        update.inserts[1],
        Leaf {
            prev_index: 2,
            ..Leaf::MAX_GUARD
        }
    );
    assert_eq!(
        update.inserts[2],
        Leaf {
            key: H256::repeat_byte(2),
            value: H256::from_low_u64_be(1),
            prev_index: 3,
            next_index: 1,
        }
    );
    assert_eq!(
        update.inserts[3],
        Leaf {
            key: H256::repeat_byte(1),
            value: H256::from_low_u64_be(2),
            prev_index: 0,
            next_index: 2,
        }
    );

    assert_eq!(update.sorted_new_leaves.len(), 4);
    assert_eq!(
        update.sorted_new_leaves[&H256::zero()],
        InsertedKeyEntry {
            index: 0,
            inserted_at: 0,
        }
    );
    assert_eq!(
        update.sorted_new_leaves[&H256::repeat_byte(0xff)],
        InsertedKeyEntry {
            index: 1,
            inserted_at: 0,
        }
    );
    assert_eq!(
        update.sorted_new_leaves[&H256::repeat_byte(2)],
        InsertedKeyEntry {
            index: 2,
            inserted_at: 0,
        }
    );
    assert_eq!(
        update.sorted_new_leaves[&H256::repeat_byte(1)],
        InsertedKeyEntry {
            index: 3,
            inserted_at: 0,
        }
    );
}

fn test_creating_empty_tree<P: TreeParams<Hasher = Blake2Hasher>>() {
    const {
        assert!(P::TREE_DEPTH == 64);
    }

    let mut patch = WorkingPatchSet::<P>::empty();
    let final_update = patch.update(TreeUpdate::for_empty_tree(&[]).unwrap());
    assert_eq!(final_update.version, 0);

    {
        let patch = patch.inner();
        assert_eq!(patch.leaves.len(), 2);
        assert_eq!(patch.leaves[&0], Leaf::MIN_GUARD);
        assert_eq!(patch.leaves[&1], Leaf::MAX_GUARD);
        let last_level = patch.internal.last().unwrap();
        assert_eq!(last_level.len(), 1);
        assert_eq!(last_level[&0].children.len(), 2);

        for level in patch.internal.iter().rev().skip(1) {
            assert_eq!(level.len(), 1);
            assert_eq!(level[&0].children.len(), 1);
        }

        assert_eq!(patch.leaf_count, 2);
        assert_eq!(patch.root().root_node.children.len(), 1);
    }

    let (patch, ..) = patch.finalize(&Blake2Hasher, final_update);
    assert_eq!(patch.manifest.version_count, 1);
    assert_eq!(patch.patches_by_version.len(), 1);
    let root = patch.try_root(0).unwrap().expect("no root");
    assert_eq!(root.leaf_count, 2);

    assert_eq!(root.root_node.children.len(), 1);
    let expected_root_hash: H256 =
        "0x8a41011d351813c31088367deecc9b70677ecf15ffc24ee450045cdeaf447f63"
            .parse()
            .unwrap();
    assert_eq!(root.hash::<P>(&Blake2Hasher), expected_root_hash);
}

#[test]
fn creating_empty_tree() {
    println!("Default tree params");
    test_creating_empty_tree::<DefaultTreeParams>();
    println!("Node depth = 3");
    test_creating_empty_tree::<DefaultTreeParams<64, 3>>();
    println!("Node depth = 2");
    test_creating_empty_tree::<DefaultTreeParams<64, 2>>();
}

fn test_creating_tree_with_leaves_in_single_batch<P>()
where
    P: TreeParams<Hasher = Blake2Hasher>,
{
    const {
        assert!(P::TREE_DEPTH == 64);
    }

    let mut patch = WorkingPatchSet::<P>::empty();
    let update = TreeUpdate::for_empty_tree(&[TreeEntry {
        key: H256::repeat_byte(0x01),
        value: H256::repeat_byte(0x10),
    }])
    .unwrap();
    let final_update = patch.update(update);

    assert_eq!(patch.inner().leaves.len(), 3);

    let (patch, ..) = patch.finalize(&Blake2Hasher, final_update);
    let root = patch.try_root(0).unwrap().expect("no root");
    assert_eq!(root.leaf_count, 3);

    let expected_root_hash: H256 =
        "0x91a1688c802dc607125d0b5e5ab4d95d89a4a4fb8cca71a122db6076cb70f8f3"
            .parse()
            .unwrap();
    assert_eq!(root.hash::<P>(&Blake2Hasher), expected_root_hash);
}

#[test]
fn creating_tree_with_leaves_in_single_batch() {
    println!("Default tree params");
    test_creating_tree_with_leaves_in_single_batch::<DefaultTreeParams>();
    println!("Node depth = 3");
    test_creating_tree_with_leaves_in_single_batch::<DefaultTreeParams<64, 3>>();
    println!("Node depth = 2");
    test_creating_tree_with_leaves_in_single_batch::<DefaultTreeParams<64, 2>>();
}

fn test_creating_tree_with_leaves_incrementally<P>()
where
    P: TreeParams<Hasher = Blake2Hasher>,
{
    const {
        assert!(P::TREE_DEPTH == 64);
    }

    let mut patch = WorkingPatchSet::<P>::empty();
    let final_update = patch.update(TreeUpdate::for_empty_tree(&[]).unwrap());
    let (patch, ..) = patch.finalize(&Blake2Hasher, final_update);

    let merkle_tree = MerkleTree::<_, P>::with_hasher(patch, Blake2Hasher).unwrap();
    let new_entry = TreeEntry {
        key: H256::repeat_byte(0x01),
        value: H256::repeat_byte(0x10),
    };
    let (mut patch, update) = merkle_tree.create_patch(0, &[new_entry]).unwrap();

    assert_eq!(patch.inner().leaf_count, 2);
    assert_eq!(
        patch.inner().leaves,
        HashMap::from([(0, Leaf::MIN_GUARD), (1, Leaf::MAX_GUARD)])
    );

    assert!(update.updates.is_empty());
    assert_eq!(update.inserts.len(), 1);
    assert_eq!(update.inserts[0].prev_index, 0);
    assert_eq!(update.inserts[0].next_index, 1);
    assert_eq!(update.sorted_new_leaves.len(), 1);
    assert_eq!(
        update.sorted_new_leaves[&new_entry.key],
        InsertedKeyEntry {
            index: 2,
            inserted_at: 1
        }
    );

    let final_update = patch.update(update);
    {
        let patch = patch.inner();
        assert_eq!(patch.leaf_count, 3);
        assert_eq!(
            patch.leaves[&0],
            Leaf {
                next_index: 2,
                ..Leaf::MIN_GUARD
            }
        );
        assert_eq!(
            patch.leaves[&1],
            Leaf {
                prev_index: 2,
                ..Leaf::MAX_GUARD
            }
        );
        assert_eq!(
            patch.leaves[&2],
            Leaf {
                key: new_entry.key,
                value: new_entry.value,
                prev_index: 0,
                next_index: 1,
            }
        );
    }

    assert_eq!(final_update.version, 1);
    let (new_patch, ..) = patch.finalize(&Blake2Hasher, final_update);
    assert_eq!(new_patch.manifest.version_count, 2);
    assert_eq!(new_patch.patches_by_version.len(), 1);
    let root = new_patch.patches_by_version[&1].root();
    let expected_root_hash: H256 =
        "0x91a1688c802dc607125d0b5e5ab4d95d89a4a4fb8cca71a122db6076cb70f8f3"
            .parse()
            .unwrap();
    assert_eq!(root.hash::<P>(&Blake2Hasher), expected_root_hash);
}

#[test]
fn creating_tree_with_leaves_incrementally() {
    println!("Default tree params");
    test_creating_tree_with_leaves_incrementally::<DefaultTreeParams>();
    println!("Node depth = 3");
    test_creating_tree_with_leaves_incrementally::<DefaultTreeParams<64, 3>>();
    println!("Node depth = 2");
    test_creating_tree_with_leaves_incrementally::<DefaultTreeParams<64, 2>>();
}

fn test_creating_tree_with_multiple_leaves_and_update<P>()
where
    P: TreeParams<Hasher = Blake2Hasher>,
{
    const {
        assert!(P::TREE_DEPTH == 64);
    }

    let mut patch = WorkingPatchSet::<P>::empty();
    let final_update = patch.update(TreeUpdate::for_empty_tree(&[]).unwrap());
    let (patch, ..) = patch.finalize(&Blake2Hasher, final_update);

    let mut merkle_tree = MerkleTree::<_, P>::with_hasher(patch, Blake2Hasher).unwrap();
    let first_entry = TreeEntry {
        key: H256::repeat_byte(0x01),
        value: H256::repeat_byte(0x10),
    };
    let second_entry = TreeEntry {
        key: H256::repeat_byte(0x02),
        value: H256::repeat_byte(0x20),
    };
    let (mut patch, update) = merkle_tree
        .create_patch(0, &[first_entry, second_entry])
        .unwrap();

    let final_update = patch.update(update);
    let (new_patch, ..) = patch.finalize(&Blake2Hasher, final_update);

    merkle_tree.db.apply_patch(new_patch).unwrap();

    let expected_root_hash: H256 =
        "0x20881c4aa37e3be665cc078db2727f0fc821bc5d9f09f053bb9a93ebd2799fcf"
            .parse()
            .unwrap();
    assert_eq!(merkle_tree.root_hash(1).unwrap(), Some(expected_root_hash));

    let updated_entry = TreeEntry {
        key: first_entry.key,
        value: H256::repeat_byte(0x33),
    };
    let (mut patch, update) = merkle_tree.create_patch(1, &[updated_entry]).unwrap();

    assert!(update.inserts.is_empty());
    assert_eq!(update.updates, [(2, updated_entry.value)]);

    {
        let patch = patch.inner();
        // `patch` should only load the updated leaf
        assert_eq!(patch.leaves.len(), 1);
        assert_eq!(patch.leaves[&2].key, updated_entry.key);
        for level in &patch.internal {
            assert_eq!(level.len(), 1, "{level:?}");
        }
    }

    let final_update = patch.update(update);
    let (new_patch, ..) = patch.finalize(&Blake2Hasher, final_update);
    merkle_tree.db.apply_patch(new_patch).unwrap();

    let expected_root_hash: H256 =
        "0x4b6bd61930a8dee1bc412d8a38780f098137be9edbf29c078546b7492748d251"
            .parse()
            .unwrap();
    assert_eq!(merkle_tree.root_hash(2).unwrap(), Some(expected_root_hash));
}

#[test]
fn creating_tree_with_multiple_leaves_and_update() {
    println!("Default tree params");
    test_creating_tree_with_multiple_leaves_and_update::<DefaultTreeParams>();
    println!("Node depth = 3");
    test_creating_tree_with_multiple_leaves_and_update::<DefaultTreeParams<64, 3>>();
    println!("Node depth = 2");
    test_creating_tree_with_multiple_leaves_and_update::<DefaultTreeParams<64, 2>>();
}

fn test_mixed_update_and_insert<P>()
where
    P: TreeParams<Hasher = Blake2Hasher>,
{
    const {
        assert!(P::TREE_DEPTH == 64);
    }

    let mut merkle_tree =
        MerkleTree::<_, P>::with_hasher(PatchSet::default(), Blake2Hasher).unwrap();
    let first_entry = TreeEntry {
        key: H256::repeat_byte(0x01),
        value: H256::repeat_byte(0x10),
    };
    merkle_tree.extend(&[first_entry]).unwrap();

    let updated_entry = TreeEntry {
        key: first_entry.key,
        value: H256::repeat_byte(0x33),
    };
    let second_entry = TreeEntry {
        key: H256::repeat_byte(0x02),
        value: H256::repeat_byte(0x20),
    };
    let (mut patch, update) = merkle_tree
        .create_patch(0, &[updated_entry, second_entry])
        .unwrap();

    assert_eq!(
        update.inserts,
        [Leaf {
            key: second_entry.key,
            value: second_entry.value,
            prev_index: 2,
            next_index: 1,
        }]
    );
    assert_eq!(update.updates, [(2, updated_entry.value)]);
    // Leaf 1 is updated as a neighbor for the inserted leaf. Leaf 0 is not updated.
    assert_eq!(
        patch.inner().leaves.keys().copied().collect::<HashSet<_>>(),
        HashSet::from([1, 2])
    );

    let final_update = patch.update(update);
    let (new_patch, ..) = patch.finalize(&Blake2Hasher, final_update);
    merkle_tree.db.apply_patch(new_patch).unwrap();

    let expected_root_hash: H256 =
        "0x4b6bd61930a8dee1bc412d8a38780f098137be9edbf29c078546b7492748d251"
            .parse()
            .unwrap();
    assert_eq!(merkle_tree.root_hash(1).unwrap(), Some(expected_root_hash));
}

#[test]
fn mixed_update_and_insert() {
    println!("Default tree params");
    test_mixed_update_and_insert::<DefaultTreeParams>();
    println!("Node depth = 3");
    test_mixed_update_and_insert::<DefaultTreeParams<64, 3>>();
    println!("Node depth = 2");
    test_mixed_update_and_insert::<DefaultTreeParams<64, 2>>();
}

use super::*;
use crate::{hasher::HasherWithStats, types::LeafNode, MerkleTree};

#[test]
#[should_panic(expected = "Tree is expected to be in the process of recovery")]
fn recovery_for_initialized_tree() {
    let mut db = PatchSet::default();
    MerkleTreeRecovery::new(&mut db, 123).finalize();
    MerkleTreeRecovery::new(db, 123);
}

#[test]
#[should_panic(expected = "Requested to recover tree version 42")]
fn recovery_for_different_version() {
    let mut db = PatchSet::default();
    MerkleTreeRecovery::new(&mut db, 123);
    MerkleTreeRecovery::new(&mut db, 42);
}

#[test]
fn recovering_empty_tree() {
    let db = MerkleTreeRecovery::new(PatchSet::default(), 42).finalize();
    let tree = MerkleTree::new(db);
    assert_eq!(tree.latest_version(), Some(42));
    assert_eq!(tree.root(42), Some(Root::Empty));
}

#[test]
fn recovering_tree_with_single_node() {
    let mut recovery = MerkleTreeRecovery::new(PatchSet::default(), 42);
    let recovery_entry = TreeEntry::new(Key::from(123), 1, ValueHash::repeat_byte(1));
    recovery.extend_linear(vec![recovery_entry]);
    let tree = MerkleTree::new(recovery.finalize());

    assert_eq!(tree.latest_version(), Some(42));
    let mut hasher = HasherWithStats::new(&Blake2Hasher);
    assert_eq!(
        tree.latest_root_hash(),
        LeafNode::new(recovery_entry).hash(&mut hasher, 0)
    );
    tree.verify_consistency(42, true).unwrap();
}

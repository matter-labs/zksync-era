//! Getters for the Merkle tree.

use crate::{
    storage::{LoadAncestorsResult, SortedKeys, WorkingPatchSet},
    types::{LeafData, Node},
    Database, Key, MerkleTree,
};

impl<DB> MerkleTree<'_, DB>
where
    DB: Database,
{
    /// Reads leaf nodes with the specified keys from the tree storage. The nodes
    /// are returned in a `Vec` in the same order as requested.
    pub fn read_leaves(&self, version: u64, leaf_keys: &[Key]) -> Vec<Option<LeafData>> {
        let Some(root) = self.db.root(version) else {
            return vec![None; leaf_keys.len()];
        };
        let sorted_keys = SortedKeys::new(leaf_keys.iter().copied());
        let mut patch_set = WorkingPatchSet::new(version, root);
        let LoadAncestorsResult {
            longest_prefixes, ..
        } = patch_set.load_ancestors(&sorted_keys, &self.db);

        leaf_keys
            .iter()
            .zip(&longest_prefixes)
            .map(|(leaf_key, longest_prefix)| {
                let node = patch_set.get(longest_prefix);
                match node {
                    Some(Node::Leaf(leaf)) if &leaf.full_key == leaf_key => Some((*leaf).into()),
                    _ => None,
                }
            })
            .collect()
    }
}

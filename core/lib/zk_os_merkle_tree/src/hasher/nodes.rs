//! Hashing for tree nodes.

use zksync_basic_types::H256;

use crate::{
    max_nibbles_for_internal_node,
    types::{InternalNode, Root},
    HashTree, TreeParams,
};

impl InternalNode {
    pub(crate) fn hash<P: TreeParams>(&self, hasher: &P::Hasher, depth: u8) -> H256 {
        assert!(depth <= max_nibbles_for_internal_node::<P>() * P::INTERNAL_NODE_DEPTH);

        let mut hashes: Vec<_> = self.children.iter().map(|child| child.hash).collect();
        for level_offset in 0..P::INTERNAL_NODE_DEPTH.min(P::TREE_DEPTH - depth) {
            let new_len = hashes.len().div_ceil(2);
            for i in 0..new_len {
                hashes[i] = if 2 * i + 1 < hashes.len() {
                    hasher.hash_branch(&hashes[2 * i], &hashes[2 * i + 1])
                } else {
                    hasher.hash_branch(
                        &hashes[2 * i],
                        &hasher.empty_subtree_hash(depth + level_offset),
                    )
                };
            }
            hashes.truncate(new_len);
        }

        assert_eq!(hashes.len(), 1);
        hashes[0]
    }
}

impl Root {
    pub(crate) fn hash<P: TreeParams>(&self, hasher: &P::Hasher) -> H256 {
        self.root_node.hash::<P>(
            hasher,
            max_nibbles_for_internal_node::<P>() * P::INTERNAL_NODE_DEPTH,
        )
    }
}

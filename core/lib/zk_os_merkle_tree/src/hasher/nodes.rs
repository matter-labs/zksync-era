//! Hashing for tree nodes.

use zksync_basic_types::H256;

use crate::{
    types::{InternalNode, Root, TREE_DEPTH},
    HashTree,
};

impl InternalNode {
    pub(crate) fn hash(&self, hasher: &dyn HashTree, depth: u8) -> H256 {
        assert!(depth <= Self::MAX_NIBBLES * Self::DEPTH);

        let mut hashes: Vec<_> = self.children.iter().map(|child| child.hash).collect();
        for level_offset in 0..Self::DEPTH.min(TREE_DEPTH - depth) {
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
    pub(crate) fn hash(&self, hasher: &dyn HashTree) -> H256 {
        self.root_node
            .hash(hasher, InternalNode::MAX_NIBBLES * InternalNode::DEPTH)
    }
}

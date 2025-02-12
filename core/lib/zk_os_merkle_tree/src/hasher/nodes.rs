//! Hashing for tree nodes.

use zksync_basic_types::H256;

use crate::{types::InternalNode, HashTree};

impl InternalNode {
    pub(crate) fn hash(&self, hasher: &dyn HashTree, depth: u8) -> H256 {
        assert!(depth <= InternalNode::MAX_NIBBLES * 4);

        let mut hashes: Vec<_> = self.child_refs().iter().map(|child| child.hash).collect();
        for level_offset in 0..4 {
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

//! Hashing for tree nodes.

use std::collections::HashMap;

use zksync_basic_types::H256;

use crate::{
    max_nibbles_for_internal_node,
    types::{InternalNode, Root},
    HashTree, TreeParams,
};

/// Internal hashes for a single `InternalNode`. Ordered by ascending depth `1..internal_node_depth`
/// where `depth == 1` is just above child refs. I.e., the last entry contains 2 hashes (unless the internal node is incomplete),
/// the penultimate one 4 hashes, etc.
#[derive(Debug, Default)]
struct InternalNodeHashes(Vec<Vec<H256>>);

impl InternalNodeHashes {
    fn get(&self, depth_in_node: u8, index_in_node: u64) -> H256 {
        self.0[usize::from(depth_in_node) - 1][index_in_node as usize]
    }
}

impl InternalNode {
    pub(crate) fn hash<P: TreeParams>(&self, hasher: &P::Hasher, depth: u8) -> H256 {
        self.hash_inner::<P>(hasher, depth, true, |_| {})
    }

    fn hash_inner<P: TreeParams>(
        &self,
        hasher: &P::Hasher,
        depth: u8,
        hash_last_level: bool,
        mut on_level: impl FnMut(&[H256]),
    ) -> H256 {
        assert!(depth <= max_nibbles_for_internal_node::<P>() * P::INTERNAL_NODE_DEPTH);

        let mut hashes: Vec<_> = self.children.iter().map(|child| child.hash).collect();
        let mut level_count = P::INTERNAL_NODE_DEPTH.min(P::TREE_DEPTH - depth);
        if !hash_last_level {
            level_count = level_count.saturating_sub(1);
        }

        for level_offset in 0..level_count {
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
            on_level(&hashes);
        }

        hashes[0]
    }

    fn internal_hashes<P: TreeParams>(&self, hasher: &P::Hasher, depth: u8) -> InternalNodeHashes {
        let mut hashes = InternalNodeHashes::default();
        self.hash_inner::<P>(hasher, depth, false, |level_hashes| {
            hashes.0.push(level_hashes.to_vec());
        });
        hashes
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

/// Internal hashes for a level of `InternalNode`s.
#[derive(Debug)]
pub(crate) struct InternalHashes<'a> {
    nodes: &'a HashMap<u64, InternalNode>,
    // TODO: `Vec<(u64, H256)>` for a level may be more efficient
    /// Ordered by ascending depth `1..internal_node_depth` where `depth == 1` is just above child refs.
    internal_hashes: HashMap<u64, InternalNodeHashes>,
    internal_node_depth: u8,
}

impl<'a> InternalHashes<'a> {
    pub(crate) fn new<P: TreeParams>(
        nodes: &'a HashMap<u64, InternalNode>,
        hasher: &P::Hasher,
        depth: u8,
    ) -> Self {
        use rayon::prelude::*;

        let internal_hashes = nodes
            .par_iter()
            .map(|(idx, node)| (*idx, node.internal_hashes::<P>(hasher, depth)))
            .collect();

        Self {
            nodes,
            internal_hashes,
            internal_node_depth: P::INTERNAL_NODE_DEPTH,
        }
    }

    pub(crate) fn get(&self, depth_in_node: u8, index_on_level: u64) -> H256 {
        let bit_shift = self.internal_node_depth - depth_in_node;
        let node_index = index_on_level >> bit_shift;
        let index_in_node = index_on_level % (1 << bit_shift);

        if depth_in_node == 0 {
            // Get the hash from a `ChildRef`
            self.nodes[&node_index]
                .child_ref(index_in_node as usize)
                .hash
        } else {
            self.internal_hashes[&node_index].get(depth_in_node, index_in_node)
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

    use super::*;
    use crate::DefaultTreeParams;

    #[test]
    fn constructing_internal_hashes() {
        let nodes = HashMap::from([(0, InternalNode::new(16, 0)), (1, InternalNode::new(7, 0))]);
        let internal_hashes = InternalHashes::new::<DefaultTreeParams>(&nodes, &Blake2Hasher, 0);

        for i in 0..(16 + 7) {
            assert_eq!(internal_hashes.get(0, i), H256::zero());
        }
        assert_eq!(internal_hashes.internal_hashes.len(), 2);

        let expected_hash = Blake2Hasher.hash_branch(&H256::zero(), &H256::zero());
        for i in 0..(8 + 3) {
            assert_eq!(internal_hashes.get(1, i), expected_hash);
        }
        let expected_boundary_hash =
            Blake2Hasher.hash_branch(&H256::zero(), &Blake2Hasher.empty_subtree_hash(0));
        assert_eq!(internal_hashes.get(1, 11), expected_boundary_hash);

        let expected_boundary_hash =
            Blake2Hasher.hash_branch(&expected_hash, &expected_boundary_hash);
        let expected_hash = Blake2Hasher.hash_branch(&expected_hash, &expected_hash);
        for i in 0..(4 + 1) {
            assert_eq!(internal_hashes.get(2, i), expected_hash);
        }
        assert_eq!(internal_hashes.get(2, 5), expected_boundary_hash);

        let expected_boundary_hash =
            Blake2Hasher.hash_branch(&expected_hash, &expected_boundary_hash);
        let expected_hash = Blake2Hasher.hash_branch(&expected_hash, &expected_hash);
        for i in 0..2 {
            assert_eq!(internal_hashes.get(3, i), expected_hash);
        }
        assert_eq!(internal_hashes.get(3, 2), expected_boundary_hash);
    }
}

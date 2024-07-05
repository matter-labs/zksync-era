//! Hash helpers for tree nodes.

use std::slice;

use crate::{
    hasher::HasherWithStats,
    types::{ChildRef, InternalNode, LeafNode, Node, Root, ValueHash, TREE_DEPTH},
    HashTree,
};

impl LeafNode {
    pub(crate) fn hash(&self, hasher: &mut HasherWithStats<'_>, level: usize) -> ValueHash {
        let hashing_iterations = TREE_DEPTH - level;
        let mut hash = hasher.hash_leaf(&self.value_hash, self.leaf_index);
        for depth in 0..hashing_iterations {
            let empty_tree_hash = hasher.empty_subtree_hash(depth);
            hash = if self.full_key.bit(depth) {
                hasher.hash_branch(&empty_tree_hash, &hash)
            } else {
                hasher.hash_branch(&hash, &empty_tree_hash)
            };
        }
        hash
    }
}

#[derive(Debug)]
pub(crate) struct MerklePath {
    current_level: usize,
    hashes: Vec<ValueHash>,
}

impl MerklePath {
    pub fn new(level: usize) -> Self {
        Self {
            current_level: level,
            hashes: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, hasher: &HasherWithStats<'_>, maybe_hash: Option<ValueHash>) {
        if let Some(hash) = maybe_hash {
            self.hashes.push(hash);
        } else if !self.hashes.is_empty() {
            let depth = TREE_DEPTH - self.current_level;
            let empty_subtree_hash = hasher.empty_subtree_hash(depth);
            self.hashes.push(empty_subtree_hash);
        }
        self.current_level -= 1;
    }

    pub fn into_inner(self) -> Vec<ValueHash> {
        debug_assert_eq!(self.current_level, 0);
        self.hashes
    }
}

/// Cache of internal node hashes in an [`InternalNode`]. This cache is only used
/// in the full tree operation mode, when Merkle proofs are obtained for each operation.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InternalNodeCache {
    // `None` corresponds to the hash of an empty subtree at the corresponding level.
    // This allows reducing the number of hash operations at the cost of additional
    // memory consumption.
    level1: [Option<ValueHash>; 2],
    level2: [Option<ValueHash>; 4],
    level3: [Option<ValueHash>; 8],
}

impl InternalNodeCache {
    #[cfg(test)]
    fn level(&self, level_in_tree: usize) -> &[Option<ValueHash>] {
        match level_in_tree {
            1 => &self.level1,
            2 => &self.level2,
            3 => &self.level3,
            _ => unreachable!(),
        }
    }

    fn set_level(&mut self, level_in_tree: usize, source: &[Option<ValueHash>]) {
        match level_in_tree {
            0 => { /* do nothing */ }
            1 => self.level1.copy_from_slice(&source[..2]),
            2 => self.level2.copy_from_slice(&source[..4]),
            3 => self.level3.copy_from_slice(&source[..8]),
            _ => unreachable!("Level in tree must be in 0..=3"),
        }
    }

    fn update_nibble(
        &mut self,
        level_hashes: &[Option<ValueHash>],
        hasher: &mut HasherWithStats<'_>,
        level: usize,
        nibble: u8,
    ) -> ValueHash {
        let mut idx = usize::from(nibble);
        let mut node_hash = None;
        let levels = [
            self.level3.as_mut_slice(),
            self.level2.as_mut_slice(),
            self.level1.as_mut_slice(),
            slice::from_mut(&mut node_hash),
        ];
        let mut level_hashes = level_hashes;

        for (level_in_tree, next_level_hashes) in (1..=4).rev().zip(levels) {
            let overall_level = level + level_in_tree;
            // Depth of a potential empty subtree rooted at the current level.
            let subtree_depth = TREE_DEPTH - overall_level;

            let left_idx = idx - idx % 2;
            let right_idx = left_idx + 1;
            let branch_hash = hasher.hash_optional_branch(
                subtree_depth,
                level_hashes[left_idx],
                level_hashes[right_idx],
            );

            idx /= 2;
            next_level_hashes[idx] = branch_hash;
            level_hashes = next_level_hashes;
        }
        node_hash.unwrap() // `unwrap()` is safe since we must have at least 1 child
    }

    fn extend_merkle_path(
        &self,
        hasher: &HasherWithStats<'_>,
        merkle_path: &mut MerklePath,
        nibble: u8,
    ) {
        let mut idx = usize::from(nibble) / 2;
        merkle_path.push(hasher, self.level3[idx ^ 1]);
        idx /= 2;
        merkle_path.push(hasher, self.level2[idx ^ 1]);
        idx /= 2;
        merkle_path.push(hasher, self.level1[idx ^ 1]);
    }
}

impl InternalNode {
    /// Hashes this tree given the 0-based level of its tip.
    fn hash_inner(
        mut level_hashes: [Option<ValueHash>; Self::CHILD_COUNT as usize],
        hasher: &mut HasherWithStats<'_>,
        level: usize,
        mut cache: Option<&mut InternalNodeCache>,
    ) -> ValueHash {
        let mut next_level_len = level_hashes.len() / 2;
        for level_in_tree in (1..=4).rev() {
            let overall_level = level + level_in_tree;
            // Depth of a potential empty subtree rooted at the current level.
            let subtree_depth = TREE_DEPTH - overall_level;

            for i in 0..next_level_len {
                level_hashes[i] = hasher.hash_optional_branch(
                    subtree_depth,
                    level_hashes[2 * i],
                    level_hashes[2 * i + 1],
                );
            }
            next_level_len /= 2;

            if let Some(cache) = cache.as_deref_mut() {
                cache.set_level(level_in_tree - 1, &level_hashes);
            }
        }
        level_hashes[0].unwrap_or_else(|| hasher.empty_subtree_hash(TREE_DEPTH - level))
    }

    pub(crate) fn hash(&self, hasher: &mut HasherWithStats<'_>, level: usize) -> ValueHash {
        Self::hash_inner(self.child_hashes(), hasher, level, None)
    }

    pub(crate) fn updater<'s, 'h>(
        &'s mut self,
        hasher: &'s mut HasherWithStats<'h>,
        level: usize,
        nibble: u8,
    ) -> InternalNodeUpdater<'s, 'h> {
        InternalNodeUpdater {
            node: self,
            hasher,
            level,
            nibble,
        }
    }
}

#[derive(Debug)]
pub(crate) struct InternalNodeUpdater<'a, 'h> {
    node: &'a mut InternalNode,
    hasher: &'a mut HasherWithStats<'h>,
    level: usize,
    nibble: u8,
}

impl InternalNodeUpdater<'_, '_> {
    /// Ensures that the child reference for the affected nibble exists. Creates a new reference
    /// with if necessary.
    pub fn ensure_child_ref(&mut self, version: u64, is_leaf: bool) {
        if let Some(child_ref) = self.node.child_ref_mut(self.nibble) {
            child_ref.version = version;
            child_ref.is_leaf = is_leaf;
        } else {
            let child_ref = if is_leaf {
                ChildRef::leaf(version)
            } else {
                ChildRef::internal(version)
            };
            self.node.insert_child_ref(self.nibble, child_ref);
        }
    }

    pub fn update_child_hash(&mut self, child_hash: ValueHash) -> ValueHash {
        let child_ref = self.node.child_ref_mut(self.nibble).unwrap();
        child_ref.hash = child_hash;
        let child_hashes = self.node.child_hashes();

        if let Some(cache) = self.node.cache_mut() {
            cache.update_nibble(&child_hashes, self.hasher, self.level, self.nibble)
        } else {
            let mut cache = Box::default();
            let node_hash =
                InternalNode::hash_inner(child_hashes, self.hasher, self.level, Some(&mut cache));
            self.node.set_cache(cache);
            node_hash
        }
    }

    pub fn extend_merkle_path(self, merkle_path: &mut MerklePath) {
        merkle_path.hashes.reserve(4);
        let adjacent_ref = self.node.child_ref(self.nibble ^ 1);
        let adjacent_hash = adjacent_ref.map(|child| child.hash);
        merkle_path.push(self.hasher, adjacent_hash);

        let cache = if let Some(cache) = self.node.cache_mut() {
            cache
        } else {
            let child_hashes = self.node.child_hashes();
            let mut cache = Box::default();
            InternalNode::hash_inner(child_hashes, self.hasher, self.level, Some(&mut cache));
            self.node.set_cache(cache)
        };
        cache.extend_merkle_path(self.hasher, merkle_path, self.nibble);
    }
}

impl Node {
    pub(crate) fn hash(&self, hasher: &mut HasherWithStats<'_>, level: usize) -> ValueHash {
        match self {
            Self::Internal(node) => node.hash(hasher, level),
            Self::Leaf(leaf) => leaf.hash(hasher, level),
        }
    }
}

impl Root {
    pub(crate) fn hash(&self, hasher: &dyn HashTree) -> ValueHash {
        let Self::Filled { node, .. } = self else {
            return hasher.empty_tree_hash();
        };
        node.hash(&mut HasherWithStats::new(&hasher), 0)
    }
}

#[cfg(test)]
mod tests {
    use zksync_crypto::hasher::{blake2::Blake2Hasher, Hasher};
    use zksync_types::H256;

    use super::*;

    fn test_internal_node_hashing(child_indexes: &[u8]) {
        println!("Testing indices: {child_indexes:?}");

        let mut internal_node = InternalNode::default();
        for &nibble in child_indexes {
            internal_node.insert_child_ref(nibble, ChildRef::leaf(1));
            internal_node.child_ref_mut(nibble).unwrap().hash = H256([nibble; 32]);
        }

        let mut hasher = HasherWithStats::new(&Blake2Hasher);
        let node_hash =
            InternalNode::hash_inner(internal_node.child_hashes(), &mut hasher, 252, None);

        // Compute the expected hash manually.
        let mut level = [hasher.empty_subtree_hash(0); 16];
        for &nibble in child_indexes {
            level[nibble as usize] = H256([nibble; 32]);
        }
        for half_len in [8, 4, 2, 1] {
            for i in 0..half_len {
                level[i] = Blake2Hasher.compress(&level[2 * i], &level[2 * i + 1]);
            }
        }

        assert_eq!(node_hash, level[0]);
    }

    #[test]
    fn hashing_internal_node() {
        for idx in 0..16 {
            test_internal_node_hashing(&[idx]);
        }
        for idx in 0..15 {
            for other_idx in (idx + 1)..16 {
                test_internal_node_hashing(&[idx, other_idx]);
            }
        }

        test_internal_node_hashing(&[5, 7, 8]);
        test_internal_node_hashing(&[8, 13, 15]);
        test_internal_node_hashing(&[0, 1, 2, 3, 5]);
        test_internal_node_hashing(&[1, 2, 3, 4, 5, 6, 7]);
        test_internal_node_hashing(&[0, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        test_internal_node_hashing(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    }

    fn test_updating_child_hash_in_internal_node(child_indexes: &[u8]) {
        let mut internal_node = InternalNode::default();
        let mut hasher = HasherWithStats::new(&Blake2Hasher);

        for (child_idx, &nibble) in child_indexes.iter().enumerate() {
            internal_node.insert_child_ref(nibble, ChildRef::leaf(1));

            let mut updater = internal_node.updater(&mut hasher, 252, nibble);
            let node_hash = updater.update_child_hash(H256([nibble; 32]));
            let mut merkle_path = MerklePath::new(TREE_DEPTH);
            updater.extend_merkle_path(&mut merkle_path);
            let merkle_path = merkle_path.hashes;
            assert!(merkle_path.len() <= 4);

            // Compute the expected hashes in the cache manually.
            let cache = *internal_node.cache_mut().unwrap();
            let mut level = [hasher.empty_subtree_hash(0); 16];
            for &nibble in &child_indexes[..=child_idx] {
                level[nibble as usize] = H256([nibble; 32]);
            }

            for (half_len, level_in_tree) in [(8, 3), (4, 2), (2, 1), (1, 0)] {
                let idx_in_merkle_path = merkle_path.len().checked_sub(level_in_tree + 1);
                let hash_from_merkle_path = idx_in_merkle_path.map(|idx| merkle_path[idx]);
                let nibble_idx = usize::from(nibble) >> (3 - level_in_tree);
                let adjacent_hash = level[nibble_idx ^ 1];
                if let Some(hash) = hash_from_merkle_path {
                    assert_eq!(hash, adjacent_hash);
                } else {
                    assert_eq!(adjacent_hash, hasher.empty_subtree_hash(3 - level_in_tree));
                }

                for i in 0..half_len {
                    level[i] = Blake2Hasher.compress(&level[2 * i], &level[2 * i + 1]);
                }

                if level_in_tree == 0 {
                    assert_eq!(node_hash, level[0]);
                } else {
                    let cache_level = cache.level(level_in_tree);
                    assert_eq!(cache_level.len(), half_len);
                    for (cached, computed) in cache_level.iter().zip(&level[..half_len]) {
                        if let Some(cached) = cached {
                            assert_eq!(cached, computed);
                        } else {
                            assert_eq!(*computed, hasher.empty_subtree_hash(4 - level_in_tree));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn updating_internal_node_cache() {
        for idx in 0..16 {
            test_updating_child_hash_in_internal_node(&[idx]);
        }
        for idx in 0..15 {
            for other_idx in (idx + 1)..16 {
                test_updating_child_hash_in_internal_node(&[idx, other_idx]);
            }
        }

        test_updating_child_hash_in_internal_node(&[5, 7, 8]);
        test_updating_child_hash_in_internal_node(&[8, 13, 15]);
        test_updating_child_hash_in_internal_node(&[0, 1, 2, 3, 5]);
        test_updating_child_hash_in_internal_node(&[1, 2, 3, 4, 5, 6, 7]);
        test_updating_child_hash_in_internal_node(&[0, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        test_updating_child_hash_in_internal_node(&[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        ]);
    }
}

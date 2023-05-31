//! Types related to DB patches: `PatchSet` and `WorkingPatchSet`.

use rayon::prelude::*;

use std::collections::HashMap;

use crate::{
    hasher::{HashTree, HashingStats},
    storage::proofs::SUBTREE_COUNT,
    types::{
        ChildRef, InternalNode, Manifest, Nibbles, NibblesBytes, Node, NodeKey, Root, ValueHash,
    },
};

/// Raw set of database changes.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(Clone))] // Used in tree consistency tests
pub struct PatchSet {
    pub(super) manifest: Manifest,
    pub(super) roots: HashMap<u64, Root>,
    pub(super) nodes_by_version: HashMap<u64, HashMap<NodeKey, Node>>,
}

impl PatchSet {
    pub(crate) fn from_manifest(manifest: Manifest) -> Self {
        Self {
            manifest,
            roots: HashMap::new(),
            nodes_by_version: HashMap::new(),
        }
    }

    pub(super) fn for_empty_root(version: u64, hasher: &dyn HashTree) -> Self {
        Self::new(version, hasher, Root::Empty, HashMap::new())
    }

    pub(super) fn new(
        version: u64,
        hasher: &dyn HashTree,
        root: Root,
        mut nodes: HashMap<NodeKey, Node>,
    ) -> Self {
        nodes.shrink_to_fit(); // We never insert into `nodes` later
        Self {
            manifest: Manifest::new(version + 1, hasher),
            roots: HashMap::from_iter([(version, root)]),
            nodes_by_version: HashMap::from_iter([(version, nodes)]),
        }
    }

    pub(super) fn is_responsible_for_version(&self, version: u64) -> bool {
        version >= self.manifest.version_count // this patch truncates `version`
            || self.roots.contains_key(&version)
    }
}

#[cfg(test)] // extensions to test tree consistency
impl PatchSet {
    pub(crate) fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
    }

    pub(crate) fn roots_mut(&mut self) -> &mut HashMap<u64, Root> {
        &mut self.roots
    }

    pub(crate) fn nodes_mut(&mut self) -> impl Iterator<Item = (&NodeKey, &mut Node)> + '_ {
        self.nodes_by_version.values_mut().flatten()
    }

    pub(crate) fn remove_node(&mut self, key: &NodeKey) {
        let nodes = self.nodes_by_version.get_mut(&key.version).unwrap();
        nodes.remove(key);
    }
}

/// Mutable version of [`PatchSet`] where we insert all changed nodes when updating
/// a Merkle tree.
#[derive(Debug)]
pub(super) struct WorkingPatchSet {
    version: u64,
    // Group changes by `nibble_count` (which is linearly tied to the tree depth:
    // `depth == nibble_count * 4`) so that we can compute hashes for all changed nodes
    // in a single traversal in `Self::finalize()`.
    changes_by_nibble_count: Vec<HashMap<NibblesBytes, Node>>,
}

impl WorkingPatchSet {
    pub fn new(version: u64, root: Root) -> Self {
        let changes_by_nibble_count = match root {
            Root::Filled { node, .. } => {
                let root_level = [(*Nibbles::EMPTY.bytes(), node)];
                vec![HashMap::from_iter(root_level)]
            }
            Root::Empty => Vec::new(),
        };
        Self {
            version,
            changes_by_nibble_count,
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn get(&self, nibbles: &Nibbles) -> Option<&Node> {
        self.changes_by_nibble_count
            .get(nibbles.nibble_count())?
            .get(nibbles.bytes())
    }

    pub fn insert(&mut self, key: Nibbles, node: Node) -> &mut Node {
        if key.nibble_count() >= self.changes_by_nibble_count.len() {
            self.changes_by_nibble_count
                .resize_with(key.nibble_count() + 1, HashMap::new);
        }

        let level = &mut self.changes_by_nibble_count[key.nibble_count()];
        level.insert(*key.bytes(), node);
        level.get_mut(key.bytes()).unwrap()
    }

    pub fn get_mut(&mut self, key: &Nibbles) -> Option<&mut Node> {
        let level = self.changes_by_nibble_count.get_mut(key.nibble_count())?;
        level.get_mut(key.bytes())
    }

    pub fn child_ref_mut(&mut self, key: &Nibbles, child_nibble: u8) -> Option<&mut ChildRef> {
        let Node::Internal(parent) = self.get_mut(key)? else {
            return None;
        };
        parent.child_ref_mut(child_nibble)
    }

    pub fn push_level(&mut self, level: HashMap<NibblesBytes, Node>) {
        self.changes_by_nibble_count.push(level);
    }

    /// Ensures that the root node in the patch set, if it exists, is an internal node. Returns
    /// a copy of the root node.
    pub fn ensure_internal_root_node(&mut self) -> InternalNode {
        match self.get(&Nibbles::EMPTY) {
            Some(Node::Internal(node)) => node.clone(),
            Some(Node::Leaf(leaf)) => {
                let leaf = *leaf;
                let first_nibble = Nibbles::nibble(&leaf.full_key, 0);
                let mut internal_node = InternalNode::default();
                internal_node.insert_child_ref(first_nibble, ChildRef::leaf(self.version));
                self.insert(Nibbles::EMPTY, internal_node.clone().into());
                self.insert(Nibbles::new(&leaf.full_key, 1), leaf.into());
                internal_node
            }
            None => {
                let internal_node = InternalNode::default();
                self.insert(Nibbles::EMPTY, internal_node.clone().into());
                internal_node
            }
        }
    }

    /// Splits this patch set by the first nibble of the contained keys.
    pub fn split(self) -> [Self; SUBTREE_COUNT] {
        let mut parts = [(); SUBTREE_COUNT].map(|_| Self {
            version: self.version,
            changes_by_nibble_count: vec![HashMap::new(); self.changes_by_nibble_count.len()],
        });

        let levels = self.changes_by_nibble_count.into_iter().enumerate();
        for (nibble_count, level) in levels {
            if nibble_count == 0 {
                // Copy the root node to all parts.
                for part in &mut parts {
                    part.changes_by_nibble_count[0] = level.clone();
                }
            } else {
                for (nibbles, node) in level {
                    let first_nibble = nibbles[0] >> 4;
                    let part = &mut parts[first_nibble as usize];
                    part.changes_by_nibble_count[nibble_count].insert(nibbles, node);
                }
            }
        }
        parts
    }

    pub fn merge(&mut self, other: Self) {
        debug_assert_eq!(self.version, other.version);

        let other_len = other.changes_by_nibble_count.len();
        if self.changes_by_nibble_count.len() < other_len {
            self.changes_by_nibble_count
                .resize_with(other_len, HashMap::new);
        }

        let it = self
            .changes_by_nibble_count
            .iter_mut()
            .zip(other.changes_by_nibble_count)
            .skip(1);
        // ^ Do not overwrite the root node; it needs to be dealt with separately anyway
        for (target_level, src_level) in it {
            let expected_new_len = target_level.len() + src_level.len();
            target_level.extend(src_level);
            debug_assert_eq!(
                target_level.len(),
                expected_new_len,
                "Cannot merge `WorkingPatchSet`s with intersecting changes"
            );
        }
    }

    /// Computes hashes and serializes this changeset.
    pub fn finalize(
        self,
        leaf_count: u64,
        hasher: &dyn HashTree,
    ) -> (ValueHash, PatchSet, HashingStats) {
        let stats = HashingStats::default();

        let mut changes_by_nibble_count = self.changes_by_nibble_count;
        if changes_by_nibble_count.is_empty() {
            // The tree is empty and there is no root present.
            let patch = PatchSet::for_empty_root(self.version, hasher);
            return (hasher.empty_tree_hash(), patch, stats);
        }
        let len = changes_by_nibble_count.iter().map(HashMap::len).sum();
        let mut patched_nodes = HashMap::with_capacity(len);

        // Compute hashes for the changed nodes with decreasing nibble count (i.e., topologically
        // sorted) and store the computed hash in the parent nodes.
        while let Some(level_changes) = changes_by_nibble_count.pop() {
            let nibble_count = changes_by_nibble_count.len();
            let tree_level = nibble_count * 4;
            // `into_par_iter()` below uses `rayon` to parallelize hash computations.
            let hashed_nodes: Vec<_> = level_changes
                .into_par_iter()
                .map_init(
                    || hasher.with_stats(&stats),
                    |hasher, (nibbles, node)| {
                        let nibbles = Nibbles::from_parts(nibbles, nibble_count);
                        (nibbles, node.hash(hasher, tree_level), node)
                    },
                )
                .collect();

            for (nibbles, node_hash, node) in hashed_nodes {
                if let Some(upper_level_changes) = changes_by_nibble_count.last_mut() {
                    let (parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
                    let parent = upper_level_changes.get_mut(parent_nibbles.bytes());
                    let Some(Node::Internal(parent)) = parent else {
                        unreachable!("Node parent must be an internal node");
                    };
                    // ^ `unwrap()`s are safe by construction: the parent of any changed node
                    // is an `InternalNode` that must be in the change set as well.
                    let self_ref = parent.child_ref_mut(last_nibble).unwrap();
                    // ^ `unwrap()` is safe by construction: the parent node must reference
                    // the currently considered child.
                    self_ref.hash = node_hash;
                } else {
                    // We're at the root node level.
                    let root = Root::new(leaf_count, node);
                    let patch = PatchSet::new(self.version, hasher, root, patched_nodes);
                    return (node_hash, patch, stats);
                }

                patched_nodes.insert(nibbles.with_version(self.version), node);
            }
        }
        unreachable!("We should have returned when the root node was encountered above");
    }

    pub fn take_root(&mut self) -> Option<Node> {
        let root_level = self.changes_by_nibble_count.get_mut(0)?;
        root_level.remove(Nibbles::EMPTY.bytes())
    }

    pub fn finalize_without_hashing(mut self, leaf_count: u64, hasher: &dyn HashTree) -> PatchSet {
        let Some(root) = self.take_root() else {
            return PatchSet::for_empty_root(self.version, hasher);
        };
        let root = Root::new(leaf_count, root);

        let levels = self.changes_by_nibble_count.drain(1..);
        let nodes = levels.enumerate().flat_map(|(i, level)| {
            let nibble_count = i + 1;
            level.into_iter().map(move |(nibbles, node)| {
                let nibbles = Nibbles::from_parts(nibbles, nibble_count);
                (nibbles.with_version(self.version), node)
            })
        });
        PatchSet::new(self.version, hasher, root, nodes.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Key, LeafNode};

    fn patch_len(patch: &WorkingPatchSet) -> usize {
        patch.changes_by_nibble_count.iter().map(HashMap::len).sum()
    }

    #[test]
    fn splitting_patch_set() {
        let mut patch = WorkingPatchSet::new(0, Root::Empty);
        let node = patch.ensure_internal_root_node();
        assert_eq!(node.child_count(), 0);

        let all_nibbles = (1_u8..=255).map(|i| {
            let key = Key::from_little_endian(&[i; 32]);
            let nibbles = Nibbles::new(&key, 2 + usize::from(i) % 4);
            // ^ We need nibble count at least 2 for all `nibbles` to be distinct.
            let leaf = LeafNode::new(key, ValueHash::zero(), i.into());
            patch.insert(nibbles, leaf.into());
            nibbles
        });
        let all_nibbles: Vec<_> = all_nibbles.collect();
        assert_eq!(patch_len(&patch), all_nibbles.len() + 1); // + root node
        let parts = patch.split();

        for (i, part) in parts.iter().enumerate() {
            let part_len = patch_len(part);
            assert!(
                (15..=17).contains(&part_len),
                "unexpected {i}th part length: {part_len}"
            );

            let first_nibble = u8::try_from(i).unwrap();
            let levels = part.changes_by_nibble_count.iter().skip(1);
            for nibbles in levels.flat_map(HashMap::keys) {
                assert_eq!(nibbles[0] >> 4, first_nibble);
            }
        }

        let merged = parts
            .into_iter()
            .reduce(|mut this, other| {
                this.merge(other);
                this
            })
            .unwrap();
        for nibbles in &all_nibbles {
            assert!(merged.get(nibbles).is_some());
        }
        assert_eq!(patch_len(&merged), all_nibbles.len() + 1);
    }
}

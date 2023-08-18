//! Storage-related logic.

mod database;
mod patch;
mod proofs;
mod rocksdb;
mod serialization;
#[cfg(test)]
mod tests;

pub(crate) use self::patch::{LoadAncestorsResult, WorkingPatchSet};
pub use self::{
    database::{Database, NodeKeys, Patched, PruneDatabase, PrunePatchSet},
    patch::PatchSet,
    rocksdb::{MerkleTreeColumnFamily, RocksDBWrapper},
};

use crate::{
    hasher::HashTree,
    metrics::{BlockTimings, LeafCountMetric, Timing, TreeUpdaterMetrics},
    types::{
        BlockOutput, ChildRef, InternalNode, Key, LeafNode, Manifest, Nibbles, Node, Root,
        TreeLogEntry, TreeTags, ValueHash,
    },
    utils::increment_counter,
};

/// Mutable storage encapsulating AR16MT update logic.
#[derive(Debug)]
struct TreeUpdater {
    metrics: TreeUpdaterMetrics,
    patch_set: WorkingPatchSet,
}

impl TreeUpdater {
    fn new(version: u64, root: Root) -> Self {
        Self {
            metrics: TreeUpdaterMetrics::default(),
            patch_set: WorkingPatchSet::new(version, root),
        }
    }

    fn set_root_node(&mut self, node: Node) {
        self.patch_set.insert(Nibbles::EMPTY, node);
    }

    /// Gets a node to be mutated.
    fn get_mut(&mut self, nibbles: &Nibbles) -> Option<&mut Node> {
        self.metrics.patch_reads += 1;
        self.patch_set.get_mut(nibbles)
    }

    fn insert_node(&mut self, nibbles: Nibbles, node: impl Into<Node>, is_new: bool) {
        let node = node.into();
        match (&node, is_new) {
            (Node::Leaf(_), false) => {
                self.metrics.update_leaf_levels(nibbles.nibble_count());
                self.metrics.moved_leaves += 1;
            }
            (Node::Leaf(_), true) => {
                self.metrics.update_leaf_levels(nibbles.nibble_count());
                self.metrics.new_leaves += 1;
            }
            (Node::Internal(_), _) => {
                debug_assert!(is_new); // internal nodes are never moved
                self.metrics.new_internal_nodes += 1;
            }
        }
        self.patch_set.insert(nibbles, node);
    }

    /// Loads ancestor nodes for all keys in `key_value_pairs`. Returns the longest prefix
    /// present in the tree currently for each inserted / updated key.
    ///
    /// # Implementation notes
    ///
    /// It may seem that the loaded leaf nodes may just increase the patch size. However,
    /// each leaf node will actually be modified by [`Self::insert()`], either by changing
    /// its `value_hash` (on full key match), or by moving the leaf node down the tree
    /// (in which case the node in the patch will be overwritten by an `InternalNode`).
    fn load_ancestors<DB: Database + ?Sized>(
        &mut self,
        sorted_keys: &SortedKeys,
        db: &DB,
    ) -> Vec<Nibbles> {
        let LoadAncestorsResult {
            longest_prefixes,
            db_reads,
        } = self.patch_set.load_ancestors(sorted_keys, db);

        self.metrics.db_reads += db_reads;
        longest_prefixes
    }

    fn traverse(&self, key: Key, parent_nibbles: &Nibbles) -> TraverseOutcome {
        for nibble_idx in parent_nibbles.nibble_count().. {
            let nibbles = Nibbles::new(&key, nibble_idx);
            match self.patch_set.get(&nibbles) {
                Some(Node::Internal(_)) => { /* continue descent */ }
                Some(Node::Leaf(leaf)) if leaf.full_key == key => {
                    return TraverseOutcome::LeafMatch(nibbles, *leaf);
                }
                Some(Node::Leaf(leaf)) => {
                    return TraverseOutcome::LeafMismatch(nibbles, *leaf);
                }
                None => return TraverseOutcome::MissingChild(nibbles),
            }
        }
        unreachable!("We must have encountered a leaf or missing node when traversing");
    }

    /// Inserts or updates a value hash for the specified `key`. This implementation
    /// is almost verbatim the algorithm described in the Jellyfish Merkle tree white paper.
    /// The algorithm from the paper is as follows:
    ///
    /// 1. Walk from the root of the tree along the inserted `key` while we can.
    /// 2. If the node we've stopped at is an internal node, it means it doesn't have
    ///   a child at the corresponding nibble from `key`. Create a new leaf node with `key` and
    ///   `value_hash` and insert it as a new child of the found internal node.
    /// 3. Else the node we've stopped is a leaf. If the full key stored in this leaf is `key`,
    ///   we just need to update `value_hash` stored in the leaf.
    /// 4. Else (the node we've stopped is a leaf with `other_key != key`) we need to create
    ///   one or more internal nodes that would contain the common prefix between `key`
    ///   and `other_key` and a "fork" where these keys differ.
    ///
    /// We change step 1 by starting not from the root, but rather from the node ancestor
    /// we've found in [`Self::load_ancestors()`] for a (moderate) performance boost. Note that
    /// due to previous `insert`ions, we may still need to perform more than 1 traversal iteration.
    ///
    /// We don't update node hashes; this would lead to a significant compute overhead (internal
    /// nodes on upper levels are updated multiple times in a block). Instead, we recompute
    /// hashes for all updated nodes in [`Self::finalize()`].
    fn insert(
        &mut self,
        key: Key,
        value_hash: ValueHash,
        parent_nibbles: &Nibbles,
        leaf_index_fn: impl FnOnce() -> u64,
    ) -> (TreeLogEntry, NewLeafData) {
        let version = self.patch_set.version();
        let traverse_outcome = self.traverse(key, parent_nibbles);
        let (log, leaf_data) = match traverse_outcome {
            TraverseOutcome::LeafMatch(nibbles, mut leaf) => {
                let log = TreeLogEntry::update(leaf.value_hash, leaf.leaf_index);
                leaf.value_hash = value_hash;
                self.patch_set.insert(nibbles, leaf.into());
                self.metrics.updated_leaves += 1;
                (log, NewLeafData::new(nibbles, leaf))
            }

            TraverseOutcome::LeafMismatch(nibbles, leaf) => {
                if let Some((parent_nibbles, last_nibble)) = nibbles.split_last() {
                    self.patch_set
                        .child_ref_mut(&parent_nibbles, last_nibble)
                        .unwrap()
                        .is_leaf = false;
                }

                let mut nibble_idx = nibbles.nibble_count();
                loop {
                    let moved_leaf_nibble = Nibbles::nibble(&leaf.full_key, nibble_idx);
                    let new_leaf_nibble = Nibbles::nibble(&key, nibble_idx);
                    let mut node = InternalNode::default();
                    if moved_leaf_nibble == new_leaf_nibble {
                        // Insert a path of internal nodes with a single child.
                        node.insert_child_ref(new_leaf_nibble, ChildRef::internal(version));
                    } else {
                        // Insert a diverging internal node with 2 children for the existing
                        // and the new leaf.
                        node.insert_child_ref(new_leaf_nibble, ChildRef::leaf(version));
                        node.insert_child_ref(moved_leaf_nibble, ChildRef::leaf(version));
                    }
                    let node_nibbles = Nibbles::new(&key, nibble_idx);
                    self.insert_node(node_nibbles, node, true);
                    if moved_leaf_nibble != new_leaf_nibble {
                        break;
                    }
                    nibble_idx += 1;
                }

                let leaf_index = leaf_index_fn();
                let new_leaf = LeafNode::new(key, value_hash, leaf_index);
                let new_leaf_nibbles = Nibbles::new(&key, nibble_idx + 1);
                let leaf_data = NewLeafData::new(new_leaf_nibbles, new_leaf);
                let moved_leaf_nibbles = Nibbles::new(&leaf.full_key, nibble_idx + 1);
                let leaf_data = leaf_data.with_adjacent_leaf(moved_leaf_nibbles, leaf);
                (TreeLogEntry::insert(leaf_index), leaf_data)
            }

            TraverseOutcome::MissingChild(nibbles) if nibbles.nibble_count() == 0 => {
                // The root is currently empty; we replace it with a leaf.
                let leaf_index = leaf_index_fn();
                debug_assert_eq!(leaf_index, 1);
                let root_leaf = LeafNode::new(key, value_hash, leaf_index);
                self.set_root_node(root_leaf.into());
                let leaf_data = NewLeafData::new(Nibbles::EMPTY, root_leaf);
                (TreeLogEntry::insert(1), leaf_data)
            }

            TraverseOutcome::MissingChild(nibbles) => {
                let (parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
                let Some(Node::Internal(parent)) = self.get_mut(&parent_nibbles) else {
                    unreachable!("Node parent must be an internal node");
                };
                parent.insert_child_ref(last_nibble, ChildRef::leaf(version));
                let leaf_index = leaf_index_fn();
                let new_leaf = LeafNode::new(key, value_hash, leaf_index);
                let leaf_data = NewLeafData::new(nibbles, new_leaf);
                (TreeLogEntry::insert(leaf_index), leaf_data)
            }
        };

        if matches!(log, TreeLogEntry::Inserted { .. }) {
            self.insert_node(leaf_data.nibbles, leaf_data.leaf, true);
        }
        if let Some((nibbles, leaf)) = leaf_data.adjacent_leaf {
            self.insert_node(nibbles, leaf, false);
        }

        // Traverse nodes up to the root level and update `ChildRef.version`.
        let mut cursor = traverse_outcome.position();
        while let Some((parent_nibbles, last_nibble)) = cursor.split_last() {
            self.patch_set
                .child_ref_mut(&parent_nibbles, last_nibble)
                .unwrap()
                .version = version;
            cursor = parent_nibbles;
        }

        (log, leaf_data)
    }
}

/// [`TreeUpdater`] together with a link to the database.
#[derive(Debug)]
pub(crate) struct Storage<'a, DB: ?Sized> {
    db: &'a DB,
    hasher: &'a dyn HashTree,
    manifest: Manifest,
    leaf_count: u64,
    updater: TreeUpdater,
}

impl<'a, DB: Database + ?Sized> Storage<'a, DB> {
    /// Creates storage for a new version of the tree.
    pub fn new(db: &'a DB, hasher: &'a dyn HashTree, version: u64) -> Self {
        let mut manifest = db.manifest().unwrap_or_default();
        if manifest.tags.is_none() {
            manifest.tags = Some(TreeTags::new(hasher));
        }
        manifest.version_count = version + 1;

        let root = if version == 0 {
            Root::Empty
        } else {
            db.root(version - 1).expect("no previous root")
        };

        Self {
            db,
            hasher,
            manifest,
            leaf_count: root.leaf_count(),
            updater: TreeUpdater::new(version, root),
        }
    }

    /// Extends the Merkle tree in the lightweight operation mode, without intermediate hash
    /// computations.
    pub fn extend(mut self, key_value_pairs: Vec<(Key, ValueHash)>) -> (BlockOutput, PatchSet) {
        let load_nodes = BlockTimings::LoadNodes.start();
        let sorted_keys = SortedKeys::new(key_value_pairs.iter().map(|(key, _)| *key));
        let parent_nibbles = self.updater.load_ancestors(&sorted_keys, self.db);
        load_nodes.report();

        let extend_patch = BlockTimings::ExtendPatch.start();
        let mut logs = Vec::with_capacity(key_value_pairs.len());
        for ((key, value_hash), parent_nibbles) in key_value_pairs.into_iter().zip(parent_nibbles) {
            let (log, _) = self.updater.insert(key, value_hash, &parent_nibbles, || {
                increment_counter(&mut self.leaf_count)
            });
            logs.push(log);
        }
        extend_patch.report();

        let leaf_count = self.leaf_count;
        let (root_hash, patch) = self.finalize();
        let output = BlockOutput {
            root_hash,
            leaf_count,
            logs,
        };
        (output, patch)
    }

    fn finalize(self) -> (ValueHash, PatchSet) {
        self.updater.metrics.report();

        let finalize_patch = BlockTimings::FinalizePatch.start();
        let (root_hash, patch, stats) =
            self.updater
                .patch_set
                .finalize(self.manifest, self.leaf_count, self.hasher);
        finalize_patch.report();
        LeafCountMetric(self.leaf_count).report();
        stats.report();

        (root_hash, patch)
    }
}

/// Sorted [`Key`]s together with their indices in the block.
#[derive(Debug)]
pub(crate) struct SortedKeys(Vec<(usize, Key)>);

impl SortedKeys {
    pub fn new(keys: impl Iterator<Item = Key>) -> Self {
        let mut keys: Vec<_> = keys.enumerate().collect();
        keys.sort_unstable_by_key(|(_, key)| *key);
        Self(keys)
    }
}

/// Outcome of traversing a tree for a specific key.
#[derive(Debug)]
enum TraverseOutcome {
    /// The matching leaf is present in the tree.
    LeafMatch(Nibbles, LeafNode),
    /// There traversal ends in a leaf with mismatched full key.
    LeafMismatch(Nibbles, LeafNode),
    /// The traversal cannot proceed because of a missing child ref in an internal node.
    MissingChild(Nibbles),
}

impl TraverseOutcome {
    /// Returns the final position during the traversal.
    fn position(&self) -> Nibbles {
        match self {
            Self::LeafMatch(nibbles, _)
            | Self::LeafMismatch(nibbles, _)
            | Self::MissingChild(nibbles) => *nibbles,
        }
    }
}

/// Information about the newly inserted / updated leaf. Can also include information about
/// an adjacent leaf moved down the tree.
#[derive(Debug)]
struct NewLeafData {
    /// Nibbles for the new leaf node.
    nibbles: Nibbles,
    /// The new leaf node.
    leaf: LeafNode,
    /// Nibbles and node for the adjacent leaf moved down the tree.
    adjacent_leaf: Option<(Nibbles, LeafNode)>,
}

impl NewLeafData {
    fn new(nibbles: Nibbles, leaf: LeafNode) -> Self {
        Self {
            nibbles,
            leaf,
            adjacent_leaf: None,
        }
    }

    fn with_adjacent_leaf(mut self, nibbles: Nibbles, leaf: LeafNode) -> Self {
        self.adjacent_leaf = Some((nibbles, leaf));
        self
    }
}

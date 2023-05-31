//! Storage-related logic.

use metrics::Unit;

use std::{mem, ops, sync::Once, time::Instant};

mod database;
mod patch;
mod proofs;
mod serialization;
#[cfg(test)]
mod tests;

pub use self::{
    database::{Database, NodeKeys, Patched, RocksDBWrapper},
    patch::PatchSet,
};

use self::patch::WorkingPatchSet;
use crate::{
    hasher::{HashTree, HashingStats},
    types::{
        BlockOutput, ChildRef, InternalNode, Key, LeafNode, Nibbles, Node, Root, TreeLogEntry,
        ValueHash,
    },
    utils::increment_counter,
};

#[derive(Debug, Clone, Copy, Default)]
struct StorageMetrics {
    // Metrics related to the AR16MT tree architecture
    new_leaves: u64,
    new_internal_nodes: u64,
    moved_leaves: u64,
    updated_leaves: u64,
    leaf_level_sum: u64,
    max_leaf_level: u64,
    // Metrics related to input instructions
    key_reads: u64,
    missing_key_reads: u64,
    db_reads: u64,
    patch_reads: u64,
}

impl StorageMetrics {
    fn describe() {
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.new_leaves",
            Unit::Count,
            "Number of new leaves inserted during tree traversal while processing a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.new_internal_nodes",
            Unit::Count,
            "Number of new internal nodes inserted during tree traversal while processing \
             a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.moved_leaves",
            Unit::Count,
            "Number of existing leaves moved to a new location while processing \
             a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.updated_leaves",
            Unit::Count,
            "Number of existing leaves updated while processing a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.avg_leaf_level",
            Unit::Count,
            "Average level of leaves moved or created while processing a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.max_leaf_level",
            Unit::Count,
            "Maximum level of leaves moved or created while processing a single block"
        );

        metrics::describe_gauge!(
            "merkle_tree.extend_patch.key_reads",
            Unit::Count,
            "Number of keys read while processing a single block (only applicable \
             to the full operation mode)"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.missing_key_reads",
            Unit::Count,
            "Number of missing keys read while processing a single block (only applicable \
             to the full operation mode)"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.db_reads",
            Unit::Count,
            "Number of nodes of previous versions read from the DB while processing \
             a single block"
        );
        metrics::describe_gauge!(
            "merkle_tree.extend_patch.patch_reads",
            Unit::Count,
            "Number of nodes of the current version re-read from the patch set while processing \
             a single block"
        );
    }

    fn update_leaf_levels(&mut self, nibble_count: usize) {
        let leaf_level = nibble_count as u64 * 4;
        self.leaf_level_sum += leaf_level;
        self.max_leaf_level = self.max_leaf_level.max(leaf_level);
    }

    #[allow(clippy::cast_precision_loss)] // unlikely to happen given magnitudes of values
    fn report(self) {
        metrics::gauge!(
            "merkle_tree.extend_patch.new_leaves",
            self.new_leaves as f64
        );
        metrics::gauge!(
            "merkle_tree.extend_patch.new_internal_nodes",
            self.new_internal_nodes as f64
        );
        metrics::gauge!(
            "merkle_tree.extend_patch.moved_leaves",
            self.moved_leaves as f64
        );
        metrics::gauge!(
            "merkle_tree.extend_patch.updated_leaves",
            self.updated_leaves as f64
        );

        let touched_leaves = self.new_leaves + self.moved_leaves;
        let avg_leaf_level = if touched_leaves > 0 {
            self.leaf_level_sum as f64 / touched_leaves as f64
        } else {
            0.0
        };
        metrics::gauge!("merkle_tree.extend_patch.avg_leaf_level", avg_leaf_level);
        metrics::gauge!(
            "merkle_tree.extend_patch.max_leaf_level",
            self.max_leaf_level as f64
        );

        if self.key_reads > 0 {
            metrics::gauge!("merkle_tree.extend_patch.key_reads", self.key_reads as f64);
        }
        if self.missing_key_reads > 0 {
            metrics::gauge!(
                "merkle_tree.extend_patch.missing_key_reads",
                self.missing_key_reads as f64
            );
        }
        metrics::gauge!("merkle_tree.extend_patch.db_reads", self.db_reads as f64);
        metrics::gauge!(
            "merkle_tree.extend_patch.patch_reads",
            self.patch_reads as f64
        );
    }
}

impl ops::AddAssign for StorageMetrics {
    fn add_assign(&mut self, rhs: Self) {
        self.new_leaves += rhs.new_leaves;
        self.new_internal_nodes += rhs.new_internal_nodes;
        self.moved_leaves += rhs.moved_leaves;
        self.updated_leaves += rhs.updated_leaves;
        self.leaf_level_sum += rhs.leaf_level_sum;
        self.max_leaf_level = self.max_leaf_level.max(rhs.max_leaf_level);

        self.key_reads += rhs.key_reads;
        self.missing_key_reads += rhs.missing_key_reads;
        self.db_reads += rhs.db_reads;
        self.patch_reads += rhs.patch_reads;
    }
}

/// Mutable storage encapsulating AR16MT update logic.
#[derive(Debug)]
struct TreeUpdater {
    metrics: StorageMetrics,
    patch_set: WorkingPatchSet,
}

impl TreeUpdater {
    fn describe_metrics() {
        metrics::describe_histogram!(
            "merkle_tree.load_nodes",
            Unit::Seconds,
            "Time spent loading tree nodes from DB per block"
        );
        metrics::describe_histogram!(
            "merkle_tree.extend_patch",
            Unit::Seconds,
            "Time spent traversing the tree and creating new nodes per block"
        );
        metrics::describe_histogram!(
            "merkle_tree.finalize_patch",
            Unit::Seconds,
            "Time spent finalizing the block (mainly hash computations)"
        );
        metrics::describe_gauge!(
            "merkle_tree.leaf_count",
            Unit::Count,
            "Current number of leaves in the tree"
        );
        StorageMetrics::describe();
        HashingStats::describe();
    }

    fn new(version: u64, root: Root) -> Self {
        static METRICS_INITIALIZER: Once = Once::new();

        METRICS_INITIALIZER.call_once(Self::describe_metrics);

        Self {
            metrics: StorageMetrics::default(),
            patch_set: WorkingPatchSet::new(version, root),
        }
    }

    fn root_node_mut(&mut self) -> Option<&mut Node> {
        self.patch_set.get_mut(&Nibbles::EMPTY)
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
    /// This method works by traversing the tree level by level. It uses [`Database::tree_nodes()`]
    /// (translating to multi-get in RocksDB) for each level to expedite node loading.
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
        let Some(Node::Internal(_)) = self.root_node_mut() else {
            return vec![Nibbles::EMPTY; sorted_keys.0.len()];
        };
        let patch_set = &mut self.patch_set;
        let version = patch_set.version();

        // Longest prefix for each key in `key_value_pairs` (i.e., what we'll return from
        // this method). `None` indicates that the longest prefix for a key is not determined yet.
        let mut longest_prefixes = vec![None; sorted_keys.0.len()];
        // Previous encountered when iterating by `sorted_keys` below.
        let mut prev_nibbles = None;
        for nibble_count in 1.. {
            // Extract `nibble_count` nibbles from each key for which we haven't found the parent
            // yet. Note that nibbles in `requested_keys` are sorted.
            let requested_keys = sorted_keys.0.iter().filter_map(|(idx, key)| {
                if longest_prefixes[*idx].is_some() {
                    return None;
                }
                let nibbles = Nibbles::new(key, nibble_count);
                let (this_parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
                // ^ `unwrap()` is safe by construction; `nibble_count` is positive
                let this_ref = patch_set.child_ref_mut(&this_parent_nibbles, last_nibble);
                let Some(this_ref) = this_ref else {
                    longest_prefixes[*idx] = Some(this_parent_nibbles);
                    return None;
                };

                // Deduplicate by `nibbles`. We do it at the end to properly
                // assign `parent_nibbles` for all keys, and before the version is updated
                // for `ChildRef`s, in order to update it only once.
                if prev_nibbles == Some(nibbles) {
                    return None;
                }
                prev_nibbles = Some(nibbles);

                // Update `ChildRef.version` for all nodes that we traverse.
                let ref_version = mem::replace(&mut this_ref.version, version);
                debug_assert!(ref_version < version);
                Some((nibbles.with_version(ref_version), this_ref.is_leaf))
            });
            let requested_keys: Vec<_> = requested_keys.collect();

            if requested_keys.is_empty() {
                break;
            }
            let new_nodes = db.tree_nodes(&requested_keys);
            self.metrics.db_reads += new_nodes.len() as u64;

            // Since we load nodes level by level, we can update `patch_set` more efficiently
            // by pushing entire `HashMap`s into `changes_by_nibble_count`.
            let level = requested_keys
                .iter()
                .zip(new_nodes)
                .map(|((key, _), node)| {
                    (*key.nibbles.bytes(), node.unwrap())
                    // ^ `unwrap()` is safe: all requested nodes are referenced by their parents
                });
            patch_set.push_level(level.collect());
        }

        // All parents must be set at this point.
        longest_prefixes.into_iter().map(Option::unwrap).collect()
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

        // Traverse nodes up to the `parent_nibbles` level and update `ChildRef.version`.
        // (For nodes before the `parent_nibbles` level, the version is updated when the nodes
        // are loaded.)
        let mut cursor = traverse_outcome.position();
        let stop_count = parent_nibbles.nibble_count();
        while let Some((parent_nibbles, last_nibble)) = cursor.split_last() {
            if parent_nibbles.nibble_count() < stop_count {
                break;
            }
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
    leaf_count: u64,
    updater: TreeUpdater,
}

impl<'a, DB: Database + ?Sized> Storage<'a, DB> {
    /// Creates storage for a new version of the tree.
    pub fn new(db: &'a DB, version: u64) -> Self {
        let root = if version == 0 {
            Root::Empty
        } else {
            db.root(version - 1).expect("no previous root")
        };

        Self {
            db,
            leaf_count: root.leaf_count(),
            updater: TreeUpdater::new(version, root),
        }
    }

    /// Extends the Merkle tree in the lightweight operation mode, without intermediate hash
    /// computations.
    pub fn extend(
        mut self,
        hasher: &dyn HashTree,
        key_value_pairs: Vec<(Key, ValueHash)>,
    ) -> (BlockOutput, PatchSet) {
        let start = Instant::now();
        let sorted_keys = SortedKeys::new(key_value_pairs.iter().map(|(key, _)| *key));
        let parent_nibbles = self.updater.load_ancestors(&sorted_keys, self.db);
        metrics::histogram!("merkle_tree.load_nodes", start.elapsed());

        let start = Instant::now();
        let mut logs = Vec::with_capacity(key_value_pairs.len());
        for ((key, value_hash), parent_nibbles) in key_value_pairs.into_iter().zip(parent_nibbles) {
            let (log, _) = self.updater.insert(key, value_hash, &parent_nibbles, || {
                increment_counter(&mut self.leaf_count)
            });
            logs.push(log);
        }
        metrics::histogram!("merkle_tree.extend_patch", start.elapsed());

        let leaf_count = self.leaf_count;
        let (root_hash, patch) = self.finalize(hasher);
        let output = BlockOutput {
            root_hash,
            leaf_count,
            logs,
        };
        (output, patch)
    }

    #[allow(clippy::cast_precision_loss)] // unlikely to happen given practical leaf counts
    fn finalize(self, hasher: &dyn HashTree) -> (ValueHash, PatchSet) {
        self.updater.metrics.report();

        let start = Instant::now();
        let (root_hash, patch, stats) = self.updater.patch_set.finalize(self.leaf_count, hasher);
        metrics::histogram!("merkle_tree.finalize_patch", start.elapsed());
        metrics::gauge!("merkle_tree.leaf_count", self.leaf_count as f64);
        stats.report();

        (root_hash, patch)
    }
}

/// Sorted [`Key`]s together with their indices in the block.
#[derive(Debug)]
struct SortedKeys(Vec<(usize, Key)>);

impl SortedKeys {
    fn new(keys: impl Iterator<Item = Key>) -> Self {
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

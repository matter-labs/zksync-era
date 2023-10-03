//! Types related to DB patches: `PatchSet` and `WorkingPatchSet`.

use rayon::prelude::*;

use std::{
    collections::{hash_map::Entry, HashMap},
    iter,
};

use crate::{
    hasher::{HashTree, HasherWithStats, MerklePath},
    metrics::HashingStats,
    storage::{proofs::SUBTREE_COUNT, SortedKeys, TraverseOutcome},
    types::{
        ChildRef, InternalNode, Key, LeafNode, Manifest, Nibbles, NibblesBytes, Node, NodeKey,
        Root, ValueHash, KEY_SIZE,
    },
    utils, Database,
};

/// Raw set of database changes.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(Clone))] // Used in tree consistency tests
pub struct PatchSet {
    pub(super) manifest: Manifest,
    pub(super) roots: HashMap<u64, Root>,
    // TODO (BFT-130): investigate most efficient ways to store key-value pairs:
    //   - `HashMap`s indexed by version
    //   - Full upper levels (i.e., `Vec<Option<Node>>`)
    pub(super) nodes_by_version: HashMap<u64, HashMap<NodeKey, Node>>,
    pub(super) stale_keys_by_version: HashMap<u64, Vec<NodeKey>>,
}

impl PatchSet {
    pub(crate) fn from_manifest(manifest: Manifest) -> Self {
        Self {
            manifest,
            roots: HashMap::new(),
            nodes_by_version: HashMap::new(),
            stale_keys_by_version: HashMap::new(),
        }
    }

    pub(super) fn for_empty_root(manifest: Manifest, version: u64) -> Self {
        let stale_keys = if let Some(prev_version) = version.checked_sub(1) {
            vec![Nibbles::EMPTY.with_version(prev_version)]
        } else {
            vec![]
        };
        Self::new(manifest, version, Root::Empty, HashMap::new(), stale_keys)
    }

    pub(super) fn new(
        manifest: Manifest,
        version: u64,
        root: Root,
        mut nodes: HashMap<NodeKey, Node>,
        mut stale_keys: Vec<NodeKey>,
    ) -> Self {
        debug_assert_eq!(manifest.version_count, version + 1);

        nodes.shrink_to_fit(); // We never insert into `nodes` later
        stale_keys.shrink_to_fit();
        Self {
            manifest,
            roots: HashMap::from_iter([(version, root)]),
            nodes_by_version: HashMap::from_iter([(version, nodes)]),
            stale_keys_by_version: HashMap::from_iter([(version, stale_keys)]),
        }
    }

    pub(super) fn is_responsible_for_version(&self, version: u64) -> bool {
        version >= self.manifest.version_count // this patch truncates `version`
            || self.roots.contains_key(&version)
    }

    /// Calculates the number of hashes in `ChildRef`s copied from the previous versions
    /// of the tree. This allows to estimate redundancy of this `PatchSet`.
    pub(super) fn copied_hashes_count(&self) -> u64 {
        let copied_hashes = self.nodes_by_version.iter().map(|(&version, nodes)| {
            let copied_hashes = nodes.values().map(|node| {
                let Node::Internal(node) = node else {
                    return 0;
                };
                let copied_hashes = node.child_refs().filter(|r| r.version < version).count();
                copied_hashes as u64
            });
            copied_hashes.sum::<u64>()
        });
        copied_hashes.sum()
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

/// [`Node`] together with a flag indicating whether it was changed in the tree version
/// being created. Unchanged nodes are filtered out when finalizing a [`WorkingPatchSet`].
#[derive(Debug, Clone)]
struct WorkingNode {
    inner: Node,
    prev_version: Option<u64>,
    is_changed: bool,
}

impl WorkingNode {
    fn unchanged(inner: Node, prev_version: u64) -> Self {
        Self {
            inner,
            prev_version: Some(prev_version),
            is_changed: false,
        }
    }

    fn changed(inner: Node, prev_version: Option<u64>) -> Self {
        Self {
            inner,
            prev_version,
            is_changed: true,
        }
    }
}

/// Result of ancestors loading.
#[derive(Debug)]
pub(crate) struct LoadAncestorsResult {
    /// The longest prefixes present in the tree currently for each requested key.
    pub longest_prefixes: Vec<Nibbles>,
    /// Number of db reads used.
    pub db_reads: u64,
}

/// Mutable version of [`PatchSet`] where we insert all changed nodes when updating
/// a Merkle tree.
#[derive(Debug)]
pub(crate) struct WorkingPatchSet {
    version: u64,
    // Group changes by `nibble_count` (which is linearly tied to the tree depth:
    // `depth == nibble_count * 4`) so that we can compute hashes for all changed nodes
    // in a single traversal in `Self::finalize()`.
    changes_by_nibble_count: Vec<HashMap<NibblesBytes, WorkingNode>>,
}

impl WorkingPatchSet {
    pub fn new(version: u64, root: Root) -> Self {
        let changes_by_nibble_count = match root {
            Root::Filled { node, .. } => {
                let root_node = WorkingNode::changed(node, version.checked_sub(1));
                let root_level = [(*Nibbles::EMPTY.bytes(), root_node)];
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
        let node = self
            .changes_by_nibble_count
            .get(nibbles.nibble_count())?
            .get(nibbles.bytes())?;
        Some(&node.inner)
    }

    pub fn insert(&mut self, key: Nibbles, node: Node) {
        if key.nibble_count() >= self.changes_by_nibble_count.len() {
            self.changes_by_nibble_count
                .resize_with(key.nibble_count() + 1, HashMap::new);
        }

        let level = &mut self.changes_by_nibble_count[key.nibble_count()];
        // We use `Entry` API to ensure that `prev_version`is correctly retained
        // in existing `WorkingNode`s.
        match level.entry(*key.bytes()) {
            Entry::Vacant(entry) => {
                entry.insert(WorkingNode::changed(node, None));
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().inner = node;
                entry.get_mut().is_changed = true;
            }
        }
    }

    /// Marks the retrieved node as changed.
    pub fn get_mut(&mut self, key: &Nibbles) -> Option<&mut Node> {
        let level = self.changes_by_nibble_count.get_mut(key.nibble_count())?;
        let node = level.get_mut(key.bytes())?;
        node.is_changed = true;
        Some(&mut node.inner)
    }

    /// Analogue of [`Self::get_mut()`] that doesn't mark the node as changed.
    /// This should only be used if the only updated part of the node is its cache.
    pub fn get_mut_without_updating(&mut self, key: &Nibbles) -> Option<&mut Node> {
        let level = self.changes_by_nibble_count.get_mut(key.nibble_count())?;
        let node = level.get_mut(key.bytes())?;
        Some(&mut node.inner)
    }

    pub fn child_ref(&self, key: &Nibbles, child_nibble: u8) -> Option<&ChildRef> {
        let Node::Internal(parent) = self.get(key)? else {
            return None;
        };
        parent.child_ref(child_nibble)
    }

    pub fn child_ref_mut(&mut self, key: &Nibbles, child_nibble: u8) -> Option<&mut ChildRef> {
        let Node::Internal(parent) = self.get_mut(key)? else {
            return None;
        };
        parent.child_ref_mut(child_nibble)
    }

    /// The pushed nodes are not marked as changed, so this method should only be used
    /// if the nodes are loaded from DB.
    fn push_level_from_db<'a>(&mut self, level: impl Iterator<Item = (&'a NodeKey, Node)>) {
        let level = level
            .map(|(key, node)| {
                let node = WorkingNode::unchanged(node, key.version);
                (*key.nibbles.bytes(), node)
            })
            .collect();
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

    fn remove_unchanged_nodes(&mut self) {
        // Do not remove the root node in any case since it has special role in finalization.
        for level in self.changes_by_nibble_count.iter_mut().skip(1) {
            level.retain(|_, node| node.is_changed);
        }
    }

    fn stale_keys(&self) -> Vec<NodeKey> {
        let levels = self.changes_by_nibble_count.iter().enumerate();
        let stale_keys = levels.flat_map(|(nibble_count, level)| {
            level.iter().filter_map(move |(nibbles, node)| {
                node.prev_version.map(|prev_version| {
                    let nibbles = Nibbles::from_parts(*nibbles, nibble_count);
                    nibbles.with_version(prev_version)
                })
            })
        });
        stale_keys.collect()
    }

    /// Computes hashes and serializes this changeset.
    pub fn finalize(
        mut self,
        manifest: Manifest,
        leaf_count: u64,
        record_stale_keys: bool,
        hasher: &dyn HashTree,
    ) -> (ValueHash, PatchSet, HashingStats) {
        self.remove_unchanged_nodes();
        let stale_keys = if record_stale_keys {
            self.stale_keys()
        } else {
            vec![]
        };
        let metrics = HashingStats::default();

        let mut changes_by_nibble_count = self.changes_by_nibble_count;
        if changes_by_nibble_count.is_empty() {
            // The tree is empty and there is no root present.
            let patch = PatchSet::for_empty_root(manifest, self.version);
            return (hasher.empty_tree_hash(), patch, metrics);
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
                    || hasher.with_stats(&metrics),
                    |hasher, (nibbles, node)| {
                        let nibbles = Nibbles::from_parts(nibbles, nibble_count);
                        (nibbles, node.inner.hash(hasher, tree_level), node)
                    },
                )
                .collect();

            for (nibbles, node_hash, node) in hashed_nodes {
                if let Some(upper_level_changes) = changes_by_nibble_count.last_mut() {
                    let (parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
                    let parent = upper_level_changes.get_mut(parent_nibbles.bytes()).unwrap();
                    let Node::Internal(parent) = &mut parent.inner else {
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
                    let root = Root::new(leaf_count, node.inner);
                    let patch =
                        PatchSet::new(manifest, self.version, root, patched_nodes, stale_keys);
                    return (node_hash, patch, metrics);
                }

                patched_nodes.insert(nibbles.with_version(self.version), node.inner);
            }
        }
        unreachable!("We should have returned when the root node was encountered above");
    }

    pub fn take_root(&mut self) -> Option<Node> {
        let root_level = self.changes_by_nibble_count.get_mut(0)?;
        let node = root_level.remove(Nibbles::EMPTY.bytes())?;
        Some(node.inner)
    }

    pub fn finalize_without_hashing(mut self, manifest: Manifest, leaf_count: u64) -> PatchSet {
        self.remove_unchanged_nodes();
        let stale_keys = self.stale_keys();

        let Some(root) = self.take_root() else {
            return PatchSet::for_empty_root(manifest, self.version);
        };
        let root = Root::new(leaf_count, root);

        let levels = self.changes_by_nibble_count.drain(1..);
        let nodes = levels.enumerate().flat_map(|(i, level)| {
            let nibble_count = i + 1;
            level.into_iter().map(move |(nibbles, node)| {
                let nibbles = Nibbles::from_parts(nibbles, nibble_count);
                (nibbles.with_version(self.version), node.inner)
            })
        });
        PatchSet::new(manifest, self.version, root, nodes.collect(), stale_keys)
    }

    /// Loads ancestor nodes for all keys in `sorted_keys`.
    ///
    /// This method works by traversing the tree level by level. It uses [`Database::tree_nodes()`]
    /// (translating to multi-get in RocksDB) for each level to expedite node loading.
    pub fn load_ancestors<DB: Database + ?Sized>(
        &mut self,
        sorted_keys: &SortedKeys,
        db: &DB,
    ) -> LoadAncestorsResult {
        let Some(Node::Internal(_)) = self.get(&Nibbles::EMPTY) else {
            return LoadAncestorsResult {
                longest_prefixes: vec![Nibbles::EMPTY; sorted_keys.0.len()],
                db_reads: 0,
            };
        };

        // Longest prefix for each key in `key_value_pairs` (i.e., what we'll return from
        // this method). `None` indicates that the longest prefix for a key is not determined yet.
        let mut longest_prefixes = vec![None; sorted_keys.0.len()];
        // Previous encountered when iterating by `sorted_keys` below.
        let mut prev_nibbles = None;
        // Cumulative number of db reads.
        let mut db_reads = 0;
        for nibble_count in 1.. {
            // Extract `nibble_count` nibbles from each key for which we haven't found the parent
            // yet. Note that nibbles in `requested_keys` are sorted.
            let requested_keys = sorted_keys.0.iter().filter_map(|(idx, key)| {
                if longest_prefixes[*idx].is_some() {
                    return None;
                }
                if nibble_count > 2 * KEY_SIZE {
                    // We have traversed to the final tree level. There's nothing to load;
                    // we just need to record the longest prefix as the full key.
                    longest_prefixes[*idx] = Some(Nibbles::new(key, 2 * KEY_SIZE));
                    return None;
                }

                let nibbles = Nibbles::new(key, nibble_count);
                let (this_parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
                // ^ `unwrap()` is safe by construction; `nibble_count` is positive
                let this_ref = self.child_ref(&this_parent_nibbles, last_nibble);
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

                Some((nibbles.with_version(this_ref.version), this_ref.is_leaf))
            });
            let requested_keys: Vec<_> = requested_keys.collect();

            if requested_keys.is_empty() {
                break;
            }
            let new_nodes = db.tree_nodes(&requested_keys);
            db_reads += new_nodes.len() as u64;

            // Since we load nodes level by level, we can update `patch_set` more efficiently
            // by pushing entire `HashMap`s into `changes_by_nibble_count`.
            let level = requested_keys
                .iter()
                .zip(new_nodes)
                .map(|((key, _), node)| {
                    (key, node.unwrap())
                    // ^ `unwrap()` is safe: all requested nodes are referenced by their parents
                });
            self.push_level_from_db(level);
        }

        // All parents must be set at this point.
        let longest_prefixes = longest_prefixes.into_iter().map(Option::unwrap).collect();

        LoadAncestorsResult {
            longest_prefixes,
            db_reads,
        }
    }

    pub(super) fn traverse(&self, key: Key, parent_nibbles: &Nibbles) -> TraverseOutcome {
        for nibble_idx in parent_nibbles.nibble_count().. {
            let nibbles = Nibbles::new(&key, nibble_idx);
            match self.get(&nibbles) {
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

    pub fn load_greatest_key<DB: Database + ?Sized>(
        &mut self,
        db: &DB,
    ) -> Option<(LeafNode, LoadAncestorsResult)> {
        let mut nibbles = Nibbles::EMPTY;
        let mut db_reads = 0;
        let greatest_leaf = loop {
            match self.get(&nibbles) {
                None => return None,
                Some(Node::Leaf(leaf)) => break *leaf,
                Some(Node::Internal(node)) => {
                    let (next_nibble, child_ref) = node.last_child_ref();
                    nibbles = nibbles.push(next_nibble).unwrap();
                    // ^ `unwrap()` is safe; there can be no internal nodes on the bottommost tree level
                    let child_key = nibbles.with_version(child_ref.version);
                    let child_node = db.tree_node(&child_key, child_ref.is_leaf).unwrap();
                    // ^ `unwrap()` is safe by construction
                    self.push_level_from_db(iter::once((&child_key, child_node)));
                    db_reads += 1;
                }
            }
        };

        let result = LoadAncestorsResult {
            longest_prefixes: vec![nibbles],
            db_reads,
        };
        Some((greatest_leaf, result))
    }

    /// Creates a Merkle proof for the specified `key`, which has given `parent_nibbles`
    /// in this patch set. `root_nibble_count` specifies to which level the proof needs to be constructed.
    pub(crate) fn create_proof(
        &mut self,
        hasher: &mut HasherWithStats<'_>,
        key: Key,
        parent_nibbles: &Nibbles,
        root_nibble_count: usize,
    ) -> (Option<LeafNode>, MerklePath) {
        let traverse_outcome = self.traverse(key, parent_nibbles);
        let merkle_path = match traverse_outcome {
            TraverseOutcome::MissingChild(_) | TraverseOutcome::LeafMatch(..) => None,
            TraverseOutcome::LeafMismatch(nibbles, leaf) => {
                // Find the level at which `leaf.full_key` and `key` diverge.
                // Note the addition of 1; e.g., if the keys differ at 0th bit, they
                // differ at level 1 of the tree.
                let diverging_level = utils::find_diverging_bit(key, leaf.full_key) + 1;
                let nibble_count = nibbles.nibble_count();
                debug_assert!(diverging_level > 4 * nibble_count);
                let mut path = MerklePath::new(diverging_level);
                // Find the hash of the existing `leaf` at the level, and include it
                // as the first hash on the Merkle path.
                let adjacent_hash = leaf.hash(hasher, diverging_level);
                path.push(hasher, Some(adjacent_hash));
                // Fill the path with empty hashes until we've reached the leaf level.
                for _ in (4 * nibble_count + 1)..diverging_level {
                    path.push(hasher, None);
                }
                Some(path)
            }
        };

        let mut nibbles = traverse_outcome.position();
        let leaf_level = nibbles.nibble_count() * 4;
        debug_assert!(leaf_level >= root_nibble_count);

        let mut merkle_path = merkle_path.unwrap_or_else(|| MerklePath::new(leaf_level));
        while let Some((parent_nibbles, last_nibble)) = nibbles.split_last() {
            if parent_nibbles.nibble_count() < root_nibble_count {
                break;
            }

            let parent = self.get_mut_without_updating(&parent_nibbles);
            let Some(Node::Internal(parent)) = parent else {
                unreachable!()
            };
            let parent_level = parent_nibbles.nibble_count() * 4;
            parent
                .updater(hasher, parent_level, last_nibble)
                .extend_merkle_path(&mut merkle_path);
            nibbles = parent_nibbles;
        }

        let leaf = match traverse_outcome {
            TraverseOutcome::MissingChild(_) | TraverseOutcome::LeafMismatch(..) => None,
            TraverseOutcome::LeafMatch(_, leaf) => Some(leaf),
        };
        (leaf, merkle_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::Storage,
        types::{Key, LeafNode},
    };

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

    #[test]
    fn loading_greatest_key() {
        // Test empty DB.
        let mut patch = WorkingPatchSet::new(0, Root::Empty);
        let load_result = patch.load_greatest_key(&PatchSet::default());
        assert!(load_result.is_none());

        // Test DB with a single entry.
        let mut db = PatchSet::default();
        let key = Key::from(1234_u64);
        let (_, patch) = Storage::new(&db, &(), 0, true).extend(vec![(key, ValueHash::zero())]);
        db.apply_patch(patch);

        let mut patch = WorkingPatchSet::new(1, db.root(0).unwrap());
        let (greatest_leaf, load_result) = patch.load_greatest_key(&db).unwrap();
        assert_eq!(greatest_leaf.full_key, key);
        assert_eq!(load_result.longest_prefixes.len(), 1);
        assert_eq!(load_result.longest_prefixes[0].nibble_count(), 0);
        assert_eq!(load_result.db_reads, 0);

        // Test DB with multiple entries.
        let other_key = Key::from_little_endian(&[0xa0; 32]);
        let (_, patch) =
            Storage::new(&db, &(), 1, true).extend(vec![(other_key, ValueHash::zero())]);
        db.apply_patch(patch);

        let mut patch = WorkingPatchSet::new(2, db.root(1).unwrap());
        let (greatest_leaf, load_result) = patch.load_greatest_key(&db).unwrap();
        assert_eq!(greatest_leaf.full_key, other_key);
        assert_eq!(load_result.longest_prefixes.len(), 1);
        assert_eq!(load_result.longest_prefixes[0].nibble_count(), 1);
        assert_eq!(load_result.db_reads, 1);

        let greater_key = Key::from_little_endian(&[0xaf; 32]);
        let (_, patch) =
            Storage::new(&db, &(), 2, true).extend(vec![(greater_key, ValueHash::zero())]);
        db.apply_patch(patch);

        let mut patch = WorkingPatchSet::new(3, db.root(2).unwrap());
        let (greatest_leaf, load_result) = patch.load_greatest_key(&db).unwrap();
        assert_eq!(greatest_leaf.full_key, greater_key);
        assert_eq!(load_result.longest_prefixes.len(), 1);
        assert_eq!(load_result.longest_prefixes[0].nibble_count(), 2);
        assert_eq!(load_result.db_reads, 2);
    }
}

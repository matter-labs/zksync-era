//! `Database` trait and its implementations.

use std::ops;

use crate::{
    errors::DeserializeError,
    storage::patch::PatchSet,
    types::{Manifest, Node, NodeKey, Root},
};

/// Slice of node keys together with an indicator whether a node at the requested key is a leaf.
/// Used in [`Database::tree_nodes()`].
pub type NodeKeys = [(NodeKey, bool)];

/// Generic database functionality. Its main implementation is [`RocksDB`].
pub trait Database: Send + Sync {
    /// Tries to read the tree [`Manifest`].
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if any.
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError>;
    /// Returns the tree manifest.
    ///
    /// # Panics
    ///
    /// Panics on deserialization errors.
    fn manifest(&self) -> Option<Manifest> {
        self.try_manifest().unwrap_or_else(|err| panic!("{err}"))
    }

    /// Tries to obtain a root from this storage.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if any.
    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError>;
    /// Obtains a root from the tree storage.
    ///
    /// # Panics
    ///
    /// Panics on deserialization errors.
    fn root(&self, version: u64) -> Option<Root> {
        self.try_root(version).unwrap_or_else(|err| panic!("{err}"))
    }

    /// Obtains a node with the specified key from the tree storage. Root nodes are obtained
    /// using [`Self::root()`], never this method.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if any.
    fn try_tree_node(&self, key: &NodeKey, is_leaf: bool)
        -> Result<Option<Node>, DeserializeError>;
    /// Obtains a node with the specified key from the tree storage.
    ///
    /// # Panics
    ///
    /// Panics on deserialization errors.
    fn tree_node(&self, key: &NodeKey, is_leaf: bool) -> Option<Node> {
        self.try_tree_node(key, is_leaf)
            .unwrap_or_else(|err| panic!("{err}"))
    }

    /// Obtains nodes with the specified keys from the tree storage. The nodes
    /// are returned in a `Vec` in the same order as requested.
    ///
    /// # Panics
    ///
    /// Panics on deserialization errors.
    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        let nodes = keys
            .iter()
            .map(|(key, is_leaf)| self.try_tree_node(key, *is_leaf));
        nodes
            .collect::<Result<_, _>>()
            .unwrap_or_else(|err| panic!("{err}"))
    }

    /// Applies changes in the `patch` to this database. This operation should be atomic.
    fn apply_patch(&mut self, patch: PatchSet);
}

impl<DB: Database + ?Sized> Database for &mut DB {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        (**self).try_manifest()
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        (**self).try_root(version)
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        (**self).try_tree_node(key, is_leaf)
    }

    fn apply_patch(&mut self, patch: PatchSet) {
        (**self).apply_patch(patch);
    }
}

impl Database for PatchSet {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        Ok(Some(self.manifest.clone()))
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        let Some(patch) = self.patches_by_version.get(&version) else {
            return Ok(None);
        };
        Ok(patch.root.clone())
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        let patch_with_node = self.patches_by_version.get(&key.version);
        let node = patch_with_node.and_then(|patch| patch.nodes.get(key));
        let Some(node) = node.cloned() else {
            return Ok(None);
        };
        debug_assert_eq!(
            matches!(node, Node::Leaf(_)),
            is_leaf,
            "Internal check failed: node at {key:?} is requested as {ty}, \
             but has the opposite type",
            ty = if is_leaf { "leaf" } else { "internal node" }
        );
        Ok(Some(node))
    }

    fn apply_patch(&mut self, mut other: PatchSet) {
        if let Some(other_updated_version) = other.updated_version {
            if let Some(updated_version) = self.updated_version {
                assert_eq!(
                    other_updated_version, updated_version,
                    "Cannot merge patches with different updated versions"
                );

                let patch = self.patches_by_version.get_mut(&updated_version).unwrap();
                let other_patch = other.patches_by_version.remove(&updated_version).unwrap();
                // ^ `unwrap()`s are safe by design.
                patch.merge(other_patch);
            } else {
                assert!(
                    self.patches_by_version.keys().all(|&ver| ver > other_updated_version),
                    "Cannot update {self:?} from {other:?}; this would break the update version invariant \
                     (the update version being lesser than all inserted versions)"
                );
                self.updated_version = Some(other_updated_version);
            }
        }

        let new_version_count = other.manifest.version_count;
        if new_version_count < self.manifest.version_count {
            // Remove obsolete sub-patches from the patch.
            self.patches_by_version
                .retain(|&version, _| version < new_version_count);
        }
        self.manifest = other.manifest;
        self.patches_by_version.extend(other.patches_by_version);
        for (version, stale_keys) in other.stale_keys_by_version {
            self.stale_keys_by_version
                .entry(version)
                .or_default()
                .extend(stale_keys);
        }
        // `PatchSet` invariants hold by construction: the updated version (if set) is still lower
        // than all other versions by design.
    }
}

/// Wrapper for a [`Database`] that also contains in-memory [`PatchSet`] on top of it.
// We need to be careful to not over-delegate to the wrapped DB when the `PatchSet` contains
// an instruction to truncate tree versions. In order to do this, we use the
// `is_responsible_for_version()` in `PatchSet`, which is based not only on the contained
// tree roots, but on the manifest as well.
#[derive(Debug)]
pub struct Patched<DB> {
    inner: DB,
    patch: Option<PatchSet>,
}

impl<DB: Database> Patched<DB> {
    /// Wraps the provided database.
    pub fn new(inner: DB) -> Self {
        Self { inner, patch: None }
    }

    pub(crate) fn patched_versions(&self) -> Vec<u64> {
        self.patch.as_ref().map_or_else(Vec::new, |patch| {
            patch.patches_by_version.keys().copied().collect()
        })
    }

    /// Returns the value from the patch and a flag whether this value is final (i.e., a DB lookup
    /// is not required).
    fn lookup_patch(&self, key: &NodeKey, is_leaf: bool) -> (Option<Node>, bool) {
        let Some(patch) = &self.patch else {
            return (None, false);
        };
        if patch.is_new_version(key.version) {
            return (patch.tree_node(key, is_leaf), true);
        }
        let could_be_in_updated_patch = patch.updated_version == Some(key.version);
        if could_be_in_updated_patch {
            // Unlike with new versions, we must look both in the update patch and in the original DB.
            if let Some(node) = patch.tree_node(key, is_leaf) {
                return (Some(node), true);
            }
        }
        (None, false)
    }

    /// Provides readonly access to the wrapped DB.
    pub(crate) fn inner(&self) -> &DB {
        &self.inner
    }

    /// Provides access to the wrapped DB. Should not be used to mutate DB data.
    pub(crate) fn inner_mut(&mut self) -> &mut DB {
        &mut self.inner
    }

    /// Flushes changes from RAM to the wrapped database.
    pub fn flush(&mut self) {
        if let Some(patch) = self.patch.take() {
            self.inner.apply_patch(patch);
        }
    }

    /// Forgets about changes held in RAM.
    pub fn reset(&mut self) {
        self.patch = None;
    }

    /// Returns the wrapped database.
    ///
    /// # Panics
    ///
    /// Panics if the database contains uncommitted changes. Call [`Self::flush()`]
    /// or [`Self::reset()`] beforehand to avoid this panic.
    pub fn into_inner(self) -> DB {
        assert!(
            self.patch.is_none(),
            "The `Patched` database contains uncommitted changes"
        );
        self.inner
    }
}

impl<DB: Database> Database for Patched<DB> {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        if let Some(patch) = &self.patch {
            Ok(Some(patch.manifest.clone()))
        } else {
            self.inner.try_manifest()
        }
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        if let Some(patch) = &self.patch {
            let has_root = patch.is_new_version(version) || patch.updated_version == Some(version);
            if has_root {
                return patch.try_root(version);
            }
        }
        self.inner.try_root(version)
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        let (patch_node, is_final) = self.lookup_patch(key, is_leaf);
        if is_final {
            Ok(patch_node)
        } else if let Some(node) = patch_node {
            Ok(Some(node))
        } else {
            self.inner.try_tree_node(key, is_leaf)
        }
    }

    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        if self.patch.is_none() {
            return self.inner.tree_nodes(keys);
        }

        let mut is_in_patch = vec![false; keys.len()];
        let mut patch_values = vec![];
        for (i, (key, is_leaf)) in keys.iter().enumerate() {
            let (patch_node, is_final) = self.lookup_patch(key, *is_leaf);
            if is_final {
                patch_values.push(patch_node);
                is_in_patch[i] = true;
            } else if let Some(node) = patch_node {
                patch_values.push(Some(node));
                is_in_patch[i] = true;
            }
        }
        let db_keys: Vec<_> = keys
            .iter()
            .zip(&is_in_patch)
            .filter_map(|(&key, &is_in_patch)| (!is_in_patch).then_some(key))
            .collect();

        let mut patch_values = patch_values.into_iter();
        let mut db_values = self.inner.tree_nodes(&db_keys).into_iter();
        let values = is_in_patch.into_iter().map(|is_in_patch| {
            if is_in_patch {
                patch_values.next().unwrap()
            } else {
                db_values.next().unwrap()
            }
        });
        values.collect()
    }

    fn apply_patch(&mut self, patch: PatchSet) {
        if let Some(existing_patch) = &mut self.patch {
            existing_patch.apply_patch(patch);
        } else {
            self.patch = Some(patch);
        }
    }
}

/// Analogue of [`PatchSet`] used when pruning past versions of the Merkle tree.
#[derive(Debug)]
pub struct PrunePatchSet {
    /// Keys that need to be removed from the tree. Logically, the version of each key
    /// should be less than `min_retained_version`.
    pub(super) pruned_node_keys: Vec<NodeKey>,
    /// Range of replacing versions for stale keys that need to be removed.
    pub(super) deleted_stale_key_versions: ops::Range<u64>,
}

impl PrunePatchSet {
    pub(crate) fn new(
        pruned_node_keys: Vec<NodeKey>,
        deleted_stale_key_versions: ops::Range<u64>,
    ) -> Self {
        Self {
            pruned_node_keys,
            deleted_stale_key_versions,
        }
    }
}

/// Functionality to prune past versions of the Merkle tree.
pub trait PruneDatabase: Database {
    /// Returns the minimum new version for stale keys present in this database, or `None`
    /// if there are no stale keys.
    fn min_stale_key_version(&self) -> Option<u64>;

    /// Returns a list of node keys obsoleted in the specified `version` of the tree.
    fn stale_keys(&self, version: u64) -> Vec<NodeKey>;

    /// Atomically prunes the tree and updates information about the minimum retained version.
    fn prune(&mut self, patch: PrunePatchSet);
}

impl<T: PruneDatabase + ?Sized> PruneDatabase for &mut T {
    fn min_stale_key_version(&self) -> Option<u64> {
        (**self).min_stale_key_version()
    }

    fn stale_keys(&self, version: u64) -> Vec<NodeKey> {
        (**self).stale_keys(version)
    }

    fn prune(&mut self, patch: PrunePatchSet) {
        (**self).prune(patch);
    }
}

impl PruneDatabase for PatchSet {
    fn min_stale_key_version(&self) -> Option<u64> {
        self.stale_keys_by_version
            .iter()
            .filter_map(|(&version, keys)| (!keys.is_empty()).then_some(version))
            .min()
    }

    fn stale_keys(&self, version: u64) -> Vec<NodeKey> {
        self.stale_keys_by_version
            .get(&version)
            .cloned()
            .unwrap_or_default()
    }

    fn prune(&mut self, patch: PrunePatchSet) {
        for key in &patch.pruned_node_keys {
            let Some(patch) = self.patches_by_version.get_mut(&key.version) else {
                continue;
            };
            if key.is_empty() {
                patch.root = None;
            } else {
                patch.nodes.remove(key);
            }
        }

        self.stale_keys_by_version
            .retain(|version, _| !patch.deleted_stale_key_versions.contains(version));
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;
    use crate::{
        storage::{
            tests::{create_patch, generate_nodes, FIRST_KEY},
            Operation,
        },
        types::{InternalNode, Nibbles},
    };

    #[test]
    fn patch_set_with_update() {
        let manifest = Manifest::new(10, &());
        let old_root = Root::new(2, Node::Internal(InternalNode::default()));
        let nodes = generate_nodes(9, &[1, 2]);
        let mut patch = PatchSet::new(
            manifest,
            9,
            old_root.clone(),
            nodes.clone(),
            vec![],
            Operation::Update,
        );

        for ver in (0..9).chain(10..20) {
            assert!(patch.root(ver).is_none());
        }
        assert_eq!(patch.root(9).unwrap(), old_root);
        let (&node_key, expected_node) = nodes.iter().next().unwrap();
        let node = patch.tree_node(&node_key, true).unwrap();
        assert_eq!(node, *expected_node);

        let new_nodes = generate_nodes(10, &[3, 4]);
        let manifest = Manifest::new(11, &());
        let new_root = Root::new(4, Node::Internal(InternalNode::default()));
        let new_patch = PatchSet::new(
            manifest,
            10,
            new_root.clone(),
            new_nodes.clone(),
            vec![],
            Operation::Insert,
        );
        patch.apply_patch(new_patch);

        for ver in (0..9).chain(11..20) {
            assert!(patch.root(ver).is_none());
        }
        assert_eq!(patch.root(9).unwrap(), old_root);
        assert_eq!(patch.root(10).unwrap(), new_root);
        let (&node_key, expected_node) = nodes.iter().next().unwrap();
        let node = patch.tree_node(&node_key, true).unwrap();
        assert_eq!(node, *expected_node);
        let (&node_key, expected_node) = new_nodes.iter().next().unwrap();
        let node = patch.tree_node(&node_key, true).unwrap();
        assert_eq!(node, *expected_node);
    }

    #[test]
    fn merging_two_update_patches() {
        let manifest = Manifest::new(10, &());
        let old_root = Root::new(2, Node::Internal(InternalNode::default()));
        let nodes = generate_nodes(9, &[1, 2]);
        let mut patch = PatchSet::new(
            manifest.clone(),
            9,
            old_root,
            nodes.clone(),
            vec![],
            Operation::Update,
        );

        let new_nodes = generate_nodes(9, &[3, 4]);
        let new_root = Root::new(4, Node::Internal(InternalNode::default()));
        let new_patch = PatchSet::new(
            manifest,
            9,
            new_root.clone(),
            new_nodes.clone(),
            vec![],
            Operation::Update,
        );
        patch.apply_patch(new_patch);

        for ver in (0..9).chain(10..20) {
            assert!(patch.root(ver).is_none());
        }
        assert_eq!(patch.root(9).unwrap(), new_root);
        for (&node_key, expected_node) in nodes.iter().chain(&new_nodes) {
            let node = patch.tree_node(&node_key, true).unwrap();
            assert_eq!(node, *expected_node);
        }
    }

    #[test]
    fn requesting_nodes_in_patched_db() {
        let root = Root::new(2, Node::Internal(InternalNode::default()));
        let old_nodes = generate_nodes(0, &[1, 2]);
        // ^ These `nodes` and `root` do not comprise a valid tree, but this is fine
        // for the purposes of this test.
        let db = create_patch(0, root, old_nodes.clone());
        let mut patched = Patched::new(db);

        let new_root = Root::new(3, Node::Internal(InternalNode::default()));
        let new_nodes = generate_nodes(1, &[3, 4, 5]);
        let patch = create_patch(1, new_root, new_nodes.clone());
        patched.apply_patch(patch);

        let (&old_key, expected_node) = old_nodes.iter().next().unwrap();
        let node = patched.tree_node(&old_key, true).unwrap();
        assert_eq!(node, *expected_node);
        let (&new_key, expected_new_node) = new_nodes.iter().next().unwrap();
        let node = patched.tree_node(&new_key, true).unwrap();
        assert_eq!(node, *expected_new_node);

        let missing_keys = [
            Nibbles::new(&FIRST_KEY, 3).with_version(0),
            Nibbles::new(&FIRST_KEY, 1).with_version(1),
            Nibbles::new(&FIRST_KEY, 7).with_version(1),
            Nibbles::new(&FIRST_KEY, 2).with_version(2),
        ];
        for missing_key in missing_keys {
            assert!(patched.tree_node(&missing_key, true).is_none());
        }

        let requested_keys = [(old_key, true), (new_key, true)];
        let nodes = patched.tree_nodes(&requested_keys);
        assert_matches!(
            nodes.as_slice(),
            [Some(a), Some(b)] if a == expected_node && b == expected_new_node
        );

        let requested_keys = [(new_key, true), (missing_keys[0], false), (old_key, true)];
        let nodes = patched.tree_nodes(&requested_keys);
        assert_matches!(
            nodes.as_slice(),
            [Some(a), None, Some(b)] if a == expected_new_node && b == expected_node
        );

        let requested_keys = missing_keys.map(|key| (key, true));
        let nodes = patched.tree_nodes(&requested_keys);
        assert_eq!(nodes.as_slice(), [None, None, None, None]);

        let requested_keys: Vec<_> = old_nodes
            .keys()
            .chain(&missing_keys)
            .chain(new_nodes.keys())
            .map(|&key| (key, true))
            .collect();
        let nodes = patched.tree_nodes(&requested_keys);

        #[rustfmt::skip] // formatting array with one item per line looks uglier
        assert_matches!(
            nodes.as_slice(),
            [Some(_), Some(_), None, None, None, None, Some(_), Some(_), Some(_)]
        );
    }

    #[test]
    fn patched_db_with_update_patch() {
        let manifest = Manifest::new(10, &());
        let old_root = Root::new(2, Node::Internal(InternalNode::default()));
        let nodes = generate_nodes(9, &[1, 2]);
        let db = PatchSet::new(
            manifest.clone(),
            9,
            old_root.clone(),
            nodes.clone(),
            vec![],
            Operation::Update,
        );
        let mut patched = Patched::new(db);

        let new_nodes = generate_nodes(9, &[3, 4]);
        let new_root = Root::new(4, Node::Internal(InternalNode::default()));
        let new_patch = PatchSet::new(
            manifest,
            9,
            new_root.clone(),
            new_nodes.clone(),
            vec![],
            Operation::Update,
        );
        patched.apply_patch(new_patch);

        for ver in (0..9).chain(10..20) {
            assert!(patched.root(ver).is_none());
        }
        assert_eq!(patched.root(9).unwrap(), new_root);
        for (&node_key, expected_node) in nodes.iter().chain(&new_nodes) {
            let node = patched.tree_node(&node_key, true).unwrap();
            assert_eq!(node, *expected_node);
        }

        let requested_keys: Vec<_> = nodes
            .keys()
            .chain(new_nodes.keys())
            .map(|&key| (key, true))
            .collect();
        let retrieved_nodes = patched.tree_nodes(&requested_keys);
        assert_eq!(retrieved_nodes.len(), requested_keys.len());
        for ((key, _), node) in requested_keys.iter().zip(retrieved_nodes) {
            assert_eq!(
                node.unwrap(),
                *nodes.get(key).unwrap_or_else(|| &new_nodes[key])
            );
        }
    }
}

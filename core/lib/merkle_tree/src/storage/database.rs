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
        Ok(self.roots.get(&version).cloned())
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        let node = self
            .nodes_by_version
            .get(&key.version)
            .and_then(|nodes| nodes.get(key));
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

    fn apply_patch(&mut self, other: PatchSet) {
        let new_version_count = other.manifest.version_count;
        if new_version_count < self.manifest.version_count {
            // Remove obsolete roots and nodes from the patch.
            self.roots.retain(|&version, _| version < new_version_count);
            self.nodes_by_version
                .retain(|&version, _| version < new_version_count);
            self.stale_keys_by_version
                .retain(|&version, _| version < new_version_count);
        }
        self.manifest = other.manifest;
        self.roots.extend(other.roots);
        self.nodes_by_version.extend(other.nodes_by_version);
        self.stale_keys_by_version
            .extend(other.stale_keys_by_version);
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
        self.patch
            .as_ref()
            .map_or_else(Vec::new, |patch| patch.roots.keys().copied().collect())
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
            if patch.is_responsible_for_version(version) {
                return Ok(patch.roots.get(&version).cloned());
            }
        }
        self.inner.try_root(version)
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        let Some(patch) = &self.patch else {
            return self.inner.try_tree_node(key, is_leaf);
        };

        if patch.is_responsible_for_version(key.version) {
            patch.try_tree_node(key, is_leaf) // take use of debug assertions
        } else {
            self.inner.try_tree_node(key, is_leaf)
        }
    }

    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        let Some(patch) = &self.patch else {
            return self.inner.tree_nodes(keys);
        };

        let mut is_in_patch = Vec::with_capacity(keys.len());
        let (patch_keys, db_keys): (Vec<_>, Vec<_>) = keys.iter().partition(|(key, _)| {
            let flag = patch.is_responsible_for_version(key.version);
            is_in_patch.push(flag);
            flag
        });

        let mut patch_values = patch.tree_nodes(&patch_keys).into_iter();
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
            if key.is_empty() {
                self.roots.remove(&key.version);
            } else if let Some(nodes) = self.nodes_by_version.get_mut(&key.version) {
                nodes.remove(key);
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
        storage::tests::{create_patch, generate_nodes, FIRST_KEY},
        types::{InternalNode, Nibbles},
    };

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
}

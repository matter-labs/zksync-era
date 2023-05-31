//! `Database` trait and its implementations.

use rayon::prelude::*;

use std::path::Path;

use crate::{
    errors::{DeserializeError, ErrorContext},
    storage::patch::PatchSet,
    types::{InternalNode, LeafNode, Manifest, Node, NodeKey, Root},
};
use zksync_storage::{
    db::{self, MerkleTreeColumnFamily},
    rocksdb::WriteBatch,
    RocksDB,
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

/// Main [`Database`] implementation wrapping a [`RocksDB`] reference.
#[derive(Debug)]
pub struct RocksDBWrapper {
    db: RocksDB,
    multi_get_chunk_size: usize,
}

impl RocksDBWrapper {
    /// Key to store the tree [`Manifest`].
    // This key must not overlap with keys for nodes; easy to see that it's true,
    // since the minimum node key is [0, 0, 0, 0, 0, 0, 0, 0].
    const MANIFEST_KEY: &'static [u8] = &[0];

    /// Creates a new wrapper, initializing RocksDB at the specified directory.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let db = RocksDB::new(db::Database::MerkleTree, path, true);
        Self::from(db)
    }

    /// Sets the chunk size for multi-get operations. The requested keys will be split
    /// into chunks of this size and requested in parallel using `rayon`. Setting chunk size
    /// to a large value (e.g., `usize::MAX`) will effectively disable parallelism.
    ///
    /// [RocksDB docs] claim that multi-get operations may be parallelized internally,
    /// but this seems to be dependent on the env; it may be the case that (single-threaded)
    /// I/O parallelization is only achieved using `liburing`, which requires enabling
    /// the `io-uring` feature of `rocksdb` crate and is only available on Linux.
    /// Thus, setting this value to around `100..1_000` can still lead to substantial
    /// performance boost (order of 2x) in some environments.
    ///
    /// [RocksDB docs]: https://github.com/facebook/rocksdb/wiki/MultiGet-Performance
    pub fn set_multi_get_chunk_size(&mut self, chunk_size: usize) {
        self.multi_get_chunk_size = chunk_size;
    }

    fn raw_node(&self, key: &[u8]) -> Option<Vec<u8>> {
        let tree_cf = self.db.cf_merkle_tree_handle(MerkleTreeColumnFamily::Tree);
        self.db
            .get_cf(tree_cf, key)
            .expect("Failed reading from RocksDB")
    }

    fn raw_nodes(&self, keys: &NodeKeys) -> Vec<Option<Vec<u8>>> {
        // `par_chunks()` below uses `rayon` to speed up multi-get I/O;
        // see `Self::set_multi_get_chunk_size()` docs for an explanation why this makes sense.
        keys.par_chunks(self.multi_get_chunk_size)
            .map(|chunk| {
                let tree_cf = self.db.cf_merkle_tree_handle(MerkleTreeColumnFamily::Tree);
                let keys = chunk.iter().map(|(key, _)| (tree_cf, key.to_db_key()));

                let results = self.db.multi_get_cf(keys);
                results
                    .into_iter()
                    .map(|result| result.expect("Failed reading from RocksDB"))
            })
            .flatten_iter()
            .collect()
    }

    fn deserialize_node(
        raw_node: &[u8],
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Node, DeserializeError> {
        // If we didn't succeed with the patch set, or the key version is old,
        // access the underlying storage.
        let node = if is_leaf {
            LeafNode::deserialize(raw_node).map(Node::Leaf)
        } else {
            InternalNode::deserialize(raw_node).map(Node::Internal)
        };
        node.map_err(|err| {
            err.with_context(if is_leaf {
                ErrorContext::Leaf(*key)
            } else {
                ErrorContext::InternalNode(*key)
            })
        })
    }

    /// Returns the wrapped RocksDB instance.
    pub fn into_inner(self) -> RocksDB {
        self.db
    }
}

impl From<RocksDB> for RocksDBWrapper {
    fn from(db: RocksDB) -> Self {
        Self {
            db,
            multi_get_chunk_size: usize::MAX,
        }
    }
}

impl Database for RocksDBWrapper {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        let Some(raw_manifest) = self.raw_node(Self::MANIFEST_KEY) else {
            return Ok(None);
        };
        Manifest::deserialize(&raw_manifest)
            .map(Some)
            .map_err(|err| err.with_context(ErrorContext::Manifest))
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        let Some(raw_root) = self.raw_node(&NodeKey::empty(version).to_db_key()) else {
            return Ok(None);
        };
        Root::deserialize(&raw_root)
            .map(Some)
            .map_err(|err| err.with_context(ErrorContext::Root(version)))
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        let Some(raw_node) = self.raw_node(&key.to_db_key()) else {
            return Ok(None);
        };
        Self::deserialize_node(&raw_node, key, is_leaf).map(Some)
    }

    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        let raw_nodes = self.raw_nodes(keys).into_iter().zip(keys);

        let nodes = raw_nodes.map(|(maybe_node, (key, is_leaf))| {
            maybe_node
                .map(|raw_node| Self::deserialize_node(&raw_node, key, *is_leaf))
                .transpose()
        });
        nodes
            .collect::<Result<_, _>>()
            .unwrap_or_else(|err| panic!("{err}"))
    }

    fn apply_patch(&mut self, patch: PatchSet) {
        let tree_cf = self.db.cf_merkle_tree_handle(MerkleTreeColumnFamily::Tree);
        let mut write_batch = WriteBatch::default();
        let mut node_bytes = Vec::with_capacity(128);
        // ^ 128 looks somewhat reasonable as node capacity

        patch.manifest.serialize(&mut node_bytes);
        write_batch.put_cf(tree_cf, Self::MANIFEST_KEY, &node_bytes);

        for (root_version, root) in patch.roots {
            node_bytes.clear();
            let root_key = NodeKey::empty(root_version);
            // Delete the key range corresponding to the entire new version. This removes
            // potential garbage left after reverting the tree to a previous version.
            let next_root_key = NodeKey::empty(root_version + 1);
            write_batch.delete_range_cf(tree_cf, root_key.to_db_key(), next_root_key.to_db_key());

            root.serialize(&mut node_bytes);
            write_batch.put_cf(tree_cf, root_key.to_db_key(), &node_bytes);
        }

        let all_nodes = patch.nodes_by_version.into_values().flatten();
        for (node_key, node) in all_nodes {
            node_bytes.clear();
            node.serialize(&mut node_bytes);
            write_batch.put_cf(tree_cf, node_key.to_db_key(), &node_bytes);
        }

        self.db
            .write(write_batch)
            .expect("Failed writing a batch to RocksDB");
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
        }
        self.manifest = other.manifest;
        self.roots.extend(other.roots);
        self.nodes_by_version.extend(other.nodes_by_version);
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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tempfile::TempDir;

    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::types::Nibbles;
    use zksync_types::{H256, U256};

    const TEST_KEY: U256 = U256([0, 0, 0, 0x_dead_beef_0000_0000]);

    fn generate_nodes(version: u64, nibble_counts: &[usize]) -> HashMap<NodeKey, Node> {
        let nodes = nibble_counts.iter().map(|&count| {
            assert_ne!(count, 0);
            let key = Nibbles::new(&TEST_KEY, count).with_version(version);
            let node = LeafNode::new(TEST_KEY, H256::zero(), count as u64);
            (key, node.into())
        });
        nodes.collect()
    }

    #[test]
    fn requesting_nodes_in_patched_db() {
        let root = Root::new(2, Node::Internal(InternalNode::default()));
        let old_nodes = generate_nodes(0, &[1, 2]);
        // ^ These `nodes` and `root` do not comprise a valid tree, but this is fine
        // for the purposes of this test.
        let db = PatchSet::new(0, &(), root, old_nodes.clone());
        let mut patched = Patched::new(db);

        let new_root = Root::new(3, Node::Internal(InternalNode::default()));
        let new_nodes = generate_nodes(1, &[3, 4, 5]);
        let patch = PatchSet::new(1, &(), new_root, new_nodes.clone());
        patched.apply_patch(patch);

        let (&old_key, expected_node) = old_nodes.iter().next().unwrap();
        let node = patched.tree_node(&old_key, true).unwrap();
        assert_eq!(node, *expected_node);
        let (&new_key, expected_new_node) = new_nodes.iter().next().unwrap();
        let node = patched.tree_node(&new_key, true).unwrap();
        assert_eq!(node, *expected_new_node);

        let missing_keys = [
            Nibbles::new(&TEST_KEY, 3).with_version(0),
            Nibbles::new(&TEST_KEY, 1).with_version(1),
            Nibbles::new(&TEST_KEY, 7).with_version(1),
            Nibbles::new(&TEST_KEY, 2).with_version(2),
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
    fn garbage_is_removed_on_db_reverts() {
        let dir = TempDir::new().expect("failed creating temporary dir for RocksDB");
        let mut db = RocksDBWrapper::new(&dir);

        // Insert some data to the database.
        let mut expected_keys = HashSet::new();
        let root = Root::new(2, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(0));
        let nodes = generate_nodes(0, &[1, 2]);
        expected_keys.extend(nodes.keys().copied());
        let patch = PatchSet::new(0, &(), root, nodes);
        db.apply_patch(patch);

        assert_contains_exactly_keys(&db, &expected_keys);

        // Overwrite data by inserting a root / nodes with the same version.
        let mut expected_keys = HashSet::new();
        let root = Root::new(3, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(0));
        let nodes = generate_nodes(0, &[3, 4, 5]);
        expected_keys.extend(nodes.keys().copied());
        let mut patch = PatchSet::new(0, &(), root, nodes);

        // Insert a new version of the tree as well.
        let root = Root::new(4, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(1));
        let nodes = generate_nodes(1, &[6]);
        expected_keys.extend(nodes.keys().copied());
        patch.apply_patch(PatchSet::new(1, &(), root, nodes));
        db.apply_patch(patch);

        assert_contains_exactly_keys(&db, &expected_keys);

        // Overwrite both versions of the tree again.
        let patch = PatchSet::new(0, &(), Root::Empty, HashMap::new());
        db.apply_patch(patch);
        let patch = PatchSet::new(1, &(), Root::Empty, HashMap::new());
        db.apply_patch(patch);

        let expected_keys = HashSet::from_iter([NodeKey::empty(0), NodeKey::empty(1)]);
        assert_contains_exactly_keys(&db, &expected_keys);
    }

    fn assert_contains_exactly_keys(db: &RocksDBWrapper, expected_keys: &HashSet<NodeKey>) {
        let cf = db.db.cf_merkle_tree_handle(MerkleTreeColumnFamily::Tree);
        let actual_keys: HashSet<_> = db
            .db
            .prefix_iterator_cf(cf, [0; 8])
            .map(|(key, _)| key)
            .collect();

        let expected_raw_keys: HashSet<_> = expected_keys
            .iter()
            .map(|key| key.to_db_key().into_boxed_slice())
            .collect();
        assert_eq!(actual_keys, expected_raw_keys);
    }
}

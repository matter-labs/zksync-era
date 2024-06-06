//! RocksDB implementation of [`Database`].

use std::{any::Any, cell::RefCell, path::Path, sync::Arc};

use anyhow::Context as _;
use rayon::prelude::*;
use thread_local::ThreadLocal;
use zksync_storage::{
    db::{NamedColumnFamily, ProfileGuard, ProfiledOperation},
    rocksdb,
    rocksdb::DBPinnableSlice,
    RocksDB,
};

use crate::{
    errors::{DeserializeError, ErrorContext},
    metrics::ApplyPatchStats,
    storage::{
        database::{PruneDatabase, PrunePatchSet},
        Database, NodeKeys, PatchSet,
    },
    types::{
        InternalNode, LeafNode, Manifest, Nibbles, Node, NodeKey, ProfiledTreeOperation, Root,
        StaleNodeKey,
    },
};

/// RocksDB column families used by the tree.
#[derive(Debug, Clone, Copy)]
pub enum MerkleTreeColumnFamily {
    /// Column family containing versioned tree information in the form of
    /// `NodeKey` -> `Node` mapping.
    Tree,
    /// Column family containing stale node keys that are eventually removed by the pruning logic.
    StaleKeys,
}

impl NamedColumnFamily for MerkleTreeColumnFamily {
    const DB_NAME: &'static str = "merkle_tree";
    const ALL: &'static [Self] = &[Self::Tree, Self::StaleKeys];

    fn name(&self) -> &'static str {
        match self {
            Self::Tree => "default",
            Self::StaleKeys => "stale_keys",
        }
    }

    fn requires_tuning(&self) -> bool {
        matches!(self, Self::Tree)
    }
}

type LocalProfiledOperation = RefCell<Option<Arc<ProfiledOperation>>>;

/// Main [`Database`] implementation wrapping a [`RocksDB`] reference.
///
/// # Cloning
///
/// The wrapper is cloneable, which works by wrapping the underlying RocksDB in an [`Arc`].
/// Thus, it is technically possible to run several [`MerkleTree`]s or [`MerkleTreePruner`]s
/// for the same RocksDB instance in parallel, but this will most probably lead to unexpected results.
/// The intended usage of cloning is to have no more than one component of each kind modifying RocksDB
/// (i.e., no more than one `MerkleTree` and no more than one `MerkleTreePruner`).
///
/// [`MerkleTree`]: crate::MerkleTree
/// [`MerkleTreePruner`]: crate::MerkleTreePruner
#[derive(Debug, Clone)]
pub struct RocksDBWrapper {
    db: RocksDB<MerkleTreeColumnFamily>,
    // We want to scope profiled operations both by the thread and by DB instance, hence the use of `ThreadLocal`
    // struct (as opposed to `thread_local!` vars).
    profiled_operation: Arc<ThreadLocal<LocalProfiledOperation>>,
    multi_get_chunk_size: usize,
}

impl RocksDBWrapper {
    /// Key to store the tree [`Manifest`].
    // This key must not overlap with keys for nodes; easy to see that it's true,
    // since the minimum node key is [0, 0, 0, 0, 0, 0, 0, 0].
    const MANIFEST_KEY: &'static [u8] = &[0];

    /// Creates a new wrapper, initializing RocksDB at the specified directory.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB I/O errors.
    pub fn new(path: &Path) -> Result<Self, rocksdb::Error> {
        Ok(Self::from(RocksDB::new(path)?))
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
    // TODO (BFT-153): Benchmark multi-get performance to find out optimal value
    pub fn set_multi_get_chunk_size(&mut self, chunk_size: usize) {
        self.multi_get_chunk_size = chunk_size;
    }

    fn raw_node(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.db
            .get_cf(MerkleTreeColumnFamily::Tree, key)
            .expect("Failed reading from RocksDB")
    }

    fn raw_nodes(&self, keys: &NodeKeys) -> Vec<Option<DBPinnableSlice<'_>>> {
        // Propagate the currently profiled operation to rayon threads used in the parallel iterator below.
        let profiled_operation = self
            .profiled_operation
            .get()
            .and_then(|cell| cell.borrow().clone());

        // `par_chunks()` below uses `rayon` to speed up multi-get I/O;
        // see `Self::set_multi_get_chunk_size()` docs for an explanation why this makes sense.
        keys.par_chunks(self.multi_get_chunk_size)
            .map(|chunk| {
                let _guard = profiled_operation
                    .as_ref()
                    .and_then(ProfiledOperation::start_profiling);
                let keys = chunk.iter().map(|(key, _)| key.to_db_key());
                let results = self.db.multi_get_cf(MerkleTreeColumnFamily::Tree, keys);
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
    pub fn into_inner(self) -> RocksDB<MerkleTreeColumnFamily> {
        self.db
    }
}

impl From<RocksDB<MerkleTreeColumnFamily>> for RocksDBWrapper {
    fn from(db: RocksDB<MerkleTreeColumnFamily>) -> Self {
        Self {
            db,
            profiled_operation: Arc::new(ThreadLocal::new()),
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

    fn start_profiling(&self, operation: ProfiledTreeOperation) -> Box<dyn Any> {
        struct Guard {
            profiled_operation: Arc<ThreadLocal<LocalProfiledOperation>>,
            _guard: ProfileGuard,
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                *self.profiled_operation.get_or_default().borrow_mut() = None;
            }
        }

        let profiled_operation = Arc::new(self.db.new_profiled_operation(operation.as_str()));
        let guard = profiled_operation.start_profiling().unwrap();
        // ^ `unwrap()` is safe: the operation has just been created
        *self.profiled_operation.get_or_default().borrow_mut() = Some(profiled_operation);
        Box::new(Guard {
            profiled_operation: self.profiled_operation.clone(),
            _guard: guard,
        })
    }

    #[allow(clippy::missing_errors_doc)] // this is a trait implementation method
    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        let tree_cf = MerkleTreeColumnFamily::Tree;
        let mut write_batch = self.db.new_write_batch();
        let mut node_bytes = Vec::with_capacity(128);
        // ^ 128 looks somewhat reasonable as node capacity

        let mut metrics = ApplyPatchStats::new(patch.copied_hashes_count());

        patch.manifest.serialize(&mut node_bytes);
        write_batch.put_cf(tree_cf, Self::MANIFEST_KEY, &node_bytes);

        for (version, sub_patch) in patch.patches_by_version {
            let is_update = patch.updated_version == Some(version);
            let root_key = NodeKey::empty(version);
            if !is_update {
                // Delete the key range corresponding to the entire new version. This removes
                // potential garbage left after reverting the tree to a previous version.
                let next_root_key = NodeKey::empty(version + 1);
                let keys_to_delete = &*root_key.to_db_key()..&*next_root_key.to_db_key();
                write_batch.delete_range_cf(tree_cf, keys_to_delete);
            }

            if let Some(root) = sub_patch.root {
                node_bytes.clear();
                root.serialize(&mut node_bytes);
                metrics.update_node_bytes(&Nibbles::EMPTY, &node_bytes);
                write_batch.put_cf(tree_cf, &root_key.to_db_key(), &node_bytes);
            }
            for (node_key, node) in sub_patch.nodes {
                node_bytes.clear();
                node.serialize(&mut node_bytes);
                metrics.update_node_bytes(&node_key.nibbles, &node_bytes);
                write_batch.put_cf(tree_cf, &node_key.to_db_key(), &node_bytes);
            }
        }

        let stale_keys_cf = MerkleTreeColumnFamily::StaleKeys;
        let all_stale_keys = patch
            .stale_keys_by_version
            .into_iter()
            .flat_map(|(version, keys)| {
                keys.into_iter()
                    .map(move |key| StaleNodeKey::new(key, version))
            });
        for replaced_key in all_stale_keys {
            write_batch.put_cf(stale_keys_cf, &replaced_key.to_db_key(), &[]);
        }

        self.db
            .write(write_batch)
            .context("Failed writing a batch to RocksDB")?;
        metrics.report();
        Ok(())
    }
}

impl PruneDatabase for RocksDBWrapper {
    fn min_stale_key_version(&self) -> Option<u64> {
        let stale_keys_cf = MerkleTreeColumnFamily::StaleKeys;
        let kv_bytes = self.db.prefix_iterator_cf(stale_keys_cf, &[]).next()?;
        let version_prefix: [u8; 8] = kv_bytes.0[..8].try_into().unwrap();
        Some(u64::from_be_bytes(version_prefix))
    }

    fn stale_keys(&self, version: u64) -> Vec<NodeKey> {
        let stale_keys_cf = MerkleTreeColumnFamily::StaleKeys;
        let version_prefix = version.to_be_bytes();
        let keys = self
            .db
            .prefix_iterator_cf(stale_keys_cf, &version_prefix)
            .map(|entry| {
                let key_bytes = entry.0;
                debug_assert_eq!(&key_bytes[..8], version_prefix);
                NodeKey::from_db_key(&key_bytes[8..])
            });
        keys.collect()
    }

    fn prune(&mut self, patch: PrunePatchSet) -> anyhow::Result<()> {
        let mut write_batch = self.db.new_write_batch();

        let tree_cf = MerkleTreeColumnFamily::Tree;
        for pruned_key in patch.pruned_node_keys {
            write_batch.delete_cf(tree_cf, &pruned_key.to_db_key());
        }

        let stale_keys_cf = MerkleTreeColumnFamily::StaleKeys;
        let first_version = &patch.deleted_stale_key_versions.start.to_be_bytes() as &[_];
        let last_version = &patch.deleted_stale_key_versions.end.to_be_bytes();
        write_batch.delete_range_cf(stale_keys_cf, first_version..last_version);

        self.db
            .write(write_batch)
            .context("Failed writing a batch to RocksDB")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use tempfile::TempDir;

    use super::*;
    use crate::storage::tests::{create_patch, generate_nodes};

    #[test]
    fn garbage_is_removed_on_db_reverts() {
        let dir = TempDir::new().expect("failed creating temporary dir for RocksDB");
        let mut db = RocksDBWrapper::new(dir.path()).unwrap();

        // Insert some data to the database.
        let mut expected_keys = HashSet::new();
        let root = Root::new(2, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(0));
        let nodes = generate_nodes(0, &[1, 2]);
        expected_keys.extend(nodes.keys().copied());
        let patch = create_patch(0, root, nodes);
        db.apply_patch(patch).unwrap();

        assert_contains_exactly_keys(&db, &expected_keys);

        // Overwrite data by inserting a root / nodes with the same version.
        let mut expected_keys = HashSet::new();
        let root = Root::new(3, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(0));
        let nodes = generate_nodes(0, &[3, 4, 5]);
        expected_keys.extend(nodes.keys().copied());
        let mut patch = create_patch(0, root, nodes);

        // Insert a new version of the tree as well.
        let root = Root::new(4, Node::Internal(InternalNode::default()));
        expected_keys.insert(NodeKey::empty(1));
        let nodes = generate_nodes(1, &[6]);
        expected_keys.extend(nodes.keys().copied());
        patch.apply_patch(create_patch(1, root, nodes)).unwrap();
        db.apply_patch(patch).unwrap();

        assert_contains_exactly_keys(&db, &expected_keys);

        // Overwrite both versions of the tree again.
        let patch = create_patch(0, Root::Empty, HashMap::new());
        db.apply_patch(patch).unwrap();
        let patch = create_patch(1, Root::Empty, HashMap::new());
        db.apply_patch(patch).unwrap();

        let expected_keys = HashSet::from_iter([NodeKey::empty(0), NodeKey::empty(1)]);
        assert_contains_exactly_keys(&db, &expected_keys);
    }

    fn assert_contains_exactly_keys(db: &RocksDBWrapper, expected_keys: &HashSet<NodeKey>) {
        let cf = MerkleTreeColumnFamily::Tree;
        let actual_keys: HashSet<_> = db
            .db
            .prefix_iterator_cf(cf, &[0; 7])
            .map(|(key, _)| key)
            .collect();

        let expected_raw_keys: HashSet<_> = expected_keys
            .iter()
            .map(|key| key.to_db_key().into_boxed_slice())
            .collect();
        assert_eq!(actual_keys, expected_raw_keys);
    }
}

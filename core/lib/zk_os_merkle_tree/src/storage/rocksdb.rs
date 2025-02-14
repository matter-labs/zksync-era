//! RocksDB implementation of [`Database`].

use std::path::Path;

use anyhow::Context as _;
use zksync_basic_types::H256;
use zksync_storage::{db::NamedColumnFamily, rocksdb, rocksdb::DBPinnableSlice, RocksDB};

use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    storage::{InsertedKeyEntry, PatchSet},
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root},
    Database, DeserializeError,
};

impl NodeKey {
    const DB_KEY_LEN: usize = 8 + 1 + 8;

    fn as_db_key(&self) -> [u8; Self::DB_KEY_LEN] {
        let mut buffer = [0_u8; Self::DB_KEY_LEN];
        buffer[..8].copy_from_slice(&self.version.to_be_bytes());
        buffer[8] = self.nibble_count;
        buffer[9..].copy_from_slice(&self.index_on_level.to_be_bytes());
        buffer
    }
}

/// RocksDB column families used by the tree.
#[derive(Debug, Clone, Copy)]
pub enum MerkleTreeColumnFamily {
    /// Column family containing versioned tree information in the form of
    /// `NodeKey` -> `Node` mapping.
    Tree,
    /// Resolves keys to (index, version) tuples.
    KeyIndices,
    // TODO: stale keys
}

impl NamedColumnFamily for MerkleTreeColumnFamily {
    const DB_NAME: &'static str = "zkos_merkle_tree";
    const ALL: &'static [Self] = &[Self::Tree, Self::KeyIndices];

    fn name(&self) -> &'static str {
        match self {
            Self::Tree => "default",
            Self::KeyIndices => "key_indices",
        }
    }

    fn requires_tuning(&self) -> bool {
        matches!(self, Self::Tree)
    }
}

/// Main [`Database`] implementation wrapping a [`RocksDB`] reference.
#[derive(Debug, Clone)]
pub struct RocksDBWrapper {
    db: RocksDB<MerkleTreeColumnFamily>,
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
    pub fn set_multi_get_chunk_size(&mut self, chunk_size: usize) {
        self.multi_get_chunk_size = chunk_size;
    }

    fn raw_node(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.db
            .get_cf(MerkleTreeColumnFamily::Tree, key)
            .expect("Failed reading from RocksDB")
    }

    pub(crate) fn raw_nodes(&self, keys: &[NodeKey]) -> Vec<Option<DBPinnableSlice<'_>>> {
        use rayon::prelude::*;

        keys.par_chunks(self.multi_get_chunk_size)
            .map(|chunk| {
                let keys = chunk.iter().map(NodeKey::as_db_key);
                let results = self.db.multi_get_cf(MerkleTreeColumnFamily::Tree, keys);
                results
                    .into_iter()
                    .map(|result| result.expect("Failed reading from RocksDB"))
            })
            .flatten_iter()
            .collect()
    }

    fn deserialize_node(raw_node: &[u8], key: &NodeKey) -> Result<Node, DeserializeError> {
        let is_leaf = key.is_leaf();
        // If we didn't succeed with the patch set, or the key version is old,
        // access the underlying storage.
        let node = if is_leaf {
            Leaf::deserialize(raw_node).map(Node::Leaf)
        } else {
            InternalNode::deserialize(raw_node).map(Node::Internal)
        };
        node.map_err(|err| err.with_context(DeserializeContext::Node(*key)))
    }

    fn lookup_key(&self, key: H256, version: u64) -> Result<KeyLookup, DeserializeError> {
        let (next_key, next_entry) = self
            .db
            .from_iterator_cf(MerkleTreeColumnFamily::KeyIndices, key.as_bytes()..)
            .find_map(|(key, raw_entry)| {
                let entry = InsertedKeyEntry::deserialize(&raw_entry)
                    .map_err(|err| err.with_context(DeserializeContext::KeyIndex(key.clone())))
                    .unwrap();
                (entry.inserted_at <= version).then(|| (H256::from_slice(&key), entry))
            })
            .expect("guards must be inserted into a tree on initialization");

        if next_key == key {
            return Ok(KeyLookup::Existing(next_entry.index));
        }

        let (prev_key, prev_entry) = self
            .db
            .to_iterator_cf(MerkleTreeColumnFamily::KeyIndices, ..=key.as_bytes())
            .find_map(|(key, raw_entry)| {
                let entry = InsertedKeyEntry::deserialize(&raw_entry)
                    .map_err(|err| err.with_context(DeserializeContext::KeyIndex(key.clone())))
                    .unwrap();
                (entry.inserted_at <= version).then(|| (H256::from_slice(&key), entry))
            })
            .expect("guards must be inserted into a tree on initialization");

        Ok(KeyLookup::Missing {
            prev_key_and_index: (prev_key, prev_entry.index),
            next_key_and_index: (next_key, next_entry.index),
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
            multi_get_chunk_size: usize::MAX,
        }
    }
}

impl Database for RocksDBWrapper {
    fn indices(&self, version: u64, keys: &[H256]) -> Result<Vec<KeyLookup>, DeserializeError> {
        use rayon::prelude::*;

        let mut results = vec![];
        keys.par_iter()
            .map(|&key| self.lookup_key(key, version))
            .collect_into_vec(&mut results);
        results.into_iter().collect()
    }

    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        let Some(raw_manifest) = self.raw_node(Self::MANIFEST_KEY) else {
            return Ok(None);
        };
        Manifest::deserialize(&raw_manifest)
            .map(Some)
            .map_err(|err| err.with_context(DeserializeContext::Manifest))
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        let node_key = NodeKey::root(version);
        let Some(raw_root) = self.raw_node(&node_key.as_db_key()) else {
            return Ok(None);
        };
        Root::deserialize(&raw_root)
            .map(Some)
            .map_err(|err| err.with_context(DeserializeContext::Node(node_key)))
    }

    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError> {
        let raw_nodes = self.raw_nodes(keys).into_iter().zip(keys);

        let nodes = raw_nodes.map(|(maybe_node, key)| {
            let raw_node = maybe_node.ok_or(DeserializeErrorKind::MissingNode)?;
            Self::deserialize_node(&raw_node, key)
        });
        nodes.collect()
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        let tree_cf = MerkleTreeColumnFamily::Tree;
        let mut write_batch = self.db.new_write_batch();
        let mut node_bytes = Vec::with_capacity(128);
        // ^ 128 looks somewhat reasonable as node capacity

        patch.manifest.serialize(&mut node_bytes);
        write_batch.put_cf(tree_cf, Self::MANIFEST_KEY, &node_bytes);

        for (key, entry) in patch.sorted_new_leaves {
            node_bytes.clear();
            entry.serialize(&mut node_bytes);
            write_batch.put_cf(
                MerkleTreeColumnFamily::KeyIndices,
                key.as_bytes(),
                &node_bytes,
            );
        }

        for (version, sub_patch) in patch.patches_by_version {
            let root_key = NodeKey::root(version);
            // Delete the key range corresponding to the entire new version. This removes
            // potential garbage left after reverting the tree to a previous version.
            let next_root_key = NodeKey::root(version + 1);
            let keys_to_delete = &root_key.as_db_key()[..]..&next_root_key.as_db_key()[..];
            write_batch.delete_range_cf(tree_cf, keys_to_delete);

            node_bytes.clear();
            sub_patch.root.serialize(&mut node_bytes);
            write_batch.put_cf(tree_cf, &root_key.as_db_key(), &node_bytes);

            for (i, level) in sub_patch.internal.into_iter().enumerate() {
                let nibble_count = i as u8 + 1;
                for (index_on_level, node) in level {
                    let node_key = NodeKey {
                        version,
                        nibble_count,
                        index_on_level,
                    };
                    node_bytes.clear();
                    node.serialize(&mut node_bytes);
                    write_batch.put_cf(tree_cf, &node_key.as_db_key(), &node_bytes);
                }
            }

            for (index_on_level, leaf) in sub_patch.leaves {
                let node_key = NodeKey {
                    version,
                    nibble_count: Leaf::NIBBLES,
                    index_on_level,
                };
                node_bytes.clear();
                leaf.serialize(&mut node_bytes);
                write_batch.put_cf(tree_cf, &node_key.as_db_key(), &node_bytes);
            }
        }

        self.db
            .write(write_batch)
            .context("Failed writing a batch to RocksDB")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        array,
        collections::{BTreeMap, HashMap},
    };

    use tempfile::TempDir;

    use super::*;
    use crate::storage::PartialPatchSet;

    #[test]
    fn looking_up_keys() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        let patch = PatchSet {
            sorted_new_leaves: BTreeMap::from([
                (
                    H256::zero(),
                    InsertedKeyEntry {
                        index: 0,
                        inserted_at: 0,
                    },
                ),
                (
                    H256::repeat_byte(0xff),
                    InsertedKeyEntry {
                        index: 1,
                        inserted_at: 0,
                    },
                ),
                (
                    H256::repeat_byte(1),
                    InsertedKeyEntry {
                        index: 2,
                        inserted_at: 1,
                    },
                ),
            ]),
            ..PatchSet::default()
        };
        db.apply_patch(patch).unwrap();

        assert_eq!(
            db.lookup_key(H256::repeat_byte(1), 0).unwrap(),
            KeyLookup::Missing {
                prev_key_and_index: (H256::zero(), 0),
                next_key_and_index: (H256::repeat_byte(0xff), 1),
            }
        );
        for version in [1, 2] {
            assert_eq!(
                db.lookup_key(H256::repeat_byte(1), version).unwrap(),
                KeyLookup::Existing(2)
            );
        }

        assert_eq!(
            db.lookup_key(H256::repeat_byte(2), 0).unwrap(),
            KeyLookup::Missing {
                prev_key_and_index: (H256::zero(), 0),
                next_key_and_index: (H256::repeat_byte(0xff), 1),
            }
        );
        for version in [1, 2] {
            assert_eq!(
                db.lookup_key(H256::repeat_byte(2), version).unwrap(),
                KeyLookup::Missing {
                    prev_key_and_index: (H256::repeat_byte(1), 2),
                    next_key_and_index: (H256::repeat_byte(0xff), 1),
                }
            );
        }

        assert_eq!(
            db.lookup_key(H256::from_low_u64_be(u64::MAX), 0).unwrap(),
            KeyLookup::Missing {
                prev_key_and_index: (H256::zero(), 0),
                next_key_and_index: (H256::repeat_byte(0xff), 1),
            }
        );
        for version in [1, 2] {
            assert_eq!(
                db.lookup_key(H256::from_low_u64_be(u64::MAX), version)
                    .unwrap(),
                KeyLookup::Missing {
                    prev_key_and_index: (H256::zero(), 0),
                    next_key_and_index: (H256::repeat_byte(1), 2),
                }
            );
        }
    }

    #[test]
    fn persisting_nodes() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        let patch = PartialPatchSet {
            root: Root {
                leaf_count: 2,
                root_node: InternalNode::new(1, 0),
            },
            internal: array::from_fn(|i| HashMap::from([(0, InternalNode::new(i + 1, 0))])),
            leaves: HashMap::from([(0, Leaf::MIN_GUARD), (1, Leaf::MAX_GUARD)]),
        };
        let patch = PatchSet {
            manifest: Manifest { version_count: 1 },
            patches_by_version: HashMap::from([(0, patch)]),
            ..PatchSet::default()
        };
        db.apply_patch(patch).unwrap();

        let manifest = db.try_manifest().unwrap().expect("no manifest");
        assert_eq!(manifest.version_count, 1);

        let root = db.try_root(0).unwrap().expect("no root");
        assert_eq!(root.leaf_count, 2);
        assert_eq!(root.root_node, InternalNode::new(1, 0));

        for nibble_count in 1..=InternalNode::MAX_NIBBLES {
            let node_key = NodeKey {
                version: 0,
                nibble_count,
                index_on_level: 0,
            };
            let nodes = db.try_nodes(&[node_key]).unwrap();
            assert_eq!(nodes.len(), 1);
            let Node::Internal(node) = &nodes[0] else {
                panic!("unexpected node: {nodes:?}");
            };
            assert_eq!(*node, InternalNode::new(nibble_count.into(), 0));
        }

        let leaf_keys = [
            NodeKey {
                version: 0,
                nibble_count: Leaf::NIBBLES,
                index_on_level: 0,
            },
            NodeKey {
                version: 0,
                nibble_count: Leaf::NIBBLES,
                index_on_level: 1,
            },
        ];
        let leaves = db.try_nodes(&leaf_keys).unwrap();
        assert_eq!(leaves.len(), 2);
        let [Node::Leaf(first_leaf), Node::Leaf(second_leaf)] = leaves.as_slice() else {
            panic!("unexpected node: {leaves:?}");
        };
        assert_eq!(*first_leaf, Leaf::MIN_GUARD);
        assert_eq!(*second_leaf, Leaf::MAX_GUARD);
    }
}

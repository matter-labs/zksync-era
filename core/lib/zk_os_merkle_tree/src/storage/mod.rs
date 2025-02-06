use std::collections::{BTreeMap, HashMap};

use zksync_basic_types::H256;

use crate::{
    errors::{DeserializeContext, DeserializeError, DeserializeErrorKind},
    types::{InternalNode, KeyLookup, Leaf, Node, NodeKey, Root},
};

/// Generic database functionality. Its main implementation is [`RocksDB`].
pub trait Database: Send + Sync {
    // TODO: batching?
    fn index(&self, key: H256, version: u64) -> Result<KeyLookup, DeserializeError>;

    /// Tries to obtain a root from this storage.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if any.
    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError>;

    /// Obtains nodes with the specified key from the storage. Root nodes must be obtained
    /// using [`Self::root()`], never this method.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if any.
    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError>;

    /// Applies changes in the `patch` to this database. This operation should be atomic.
    ///
    /// # Errors
    ///
    /// Returns I/O errors.
    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()>;
}

#[derive(Debug)]
struct PartialPatchSet {
    root: Root,
    /// Offset by 1 (i.e., `internal[0]` corresponds to 1 nibble. `Vec`s are sorted by the index.
    internal: [Vec<(u64, InternalNode)>; InternalNode::MAX_NIBBLES as usize - 1],
    /// Sorted by the index.
    leaves: Vec<(u64, Leaf)>,
}

impl PartialPatchSet {
    fn node(&self, nibble_count: u8, index_on_level: u64) -> Option<Node> {
        Some(if nibble_count == Leaf::NIBBLES {
            let i = self
                .leaves
                .binary_search_by_key(&index_on_level, |(idx, _)| *idx)
                .ok()?;
            self.leaves[i].1.into()
        } else {
            let level = &self.internal[usize::from(nibble_count) - 1];
            let i = level
                .binary_search_by_key(&index_on_level, |(idx, _)| *idx)
                .ok()?;
            level[i].1.clone().into()
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct InsertedKeyEntry {
    index: u64,
    inserted_at: u64,
}

#[derive(Debug)]
pub struct PatchSet {
    patches_by_version: HashMap<u64, PartialPatchSet>,
    // We maintain a joint index for all versions to make it easier to use `PatchSet` as a `Database` or in a `Patched` wrapper.
    new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    // TODO: stale keys
}

impl Database for PatchSet {
    fn index(&self, key: H256, version: u64) -> Result<KeyLookup, DeserializeError> {
        let (next_key, next_entry) = self
            .new_leaves
            .range(key..)
            .find(|(_, entry)| entry.inserted_at <= version)
            .expect("guards must be inserted into a tree on initialization");
        if *next_key == key {
            return Ok(KeyLookup::Existing(next_entry.index));
        }

        let (prev_key, prev_entry) = self
            .new_leaves
            .range(..key)
            .rev()
            .find(|(_, entry)| entry.inserted_at <= version)
            .expect("guards must be inserted into a tree on initialization");
        Ok(KeyLookup::Missing {
            prev_key_and_index: (*prev_key, prev_entry.index),
            next_key_and_index: (*next_key, next_entry.index),
        })
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        Ok(self
            .patches_by_version
            .get(&version)
            .map(|patch| patch.root.clone()))
    }

    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError> {
        let nodes = keys.iter().map(|key| {
            assert!(key.nibble_count > 0);
            assert!(key.nibble_count <= Leaf::NIBBLES);

            let maybe_node = self
                .patches_by_version
                .get(&key.version)
                .and_then(|patch| patch.node(key.nibble_count, key.index_on_level));
            maybe_node.ok_or_else(|| {
                DeserializeError::from(DeserializeErrorKind::MissingNode)
                    .with_context(DeserializeContext::Node(*key))
            })
        });
        nodes.collect()
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        self.patches_by_version.extend(patch.patches_by_version);
        self.new_leaves.extend(patch.new_leaves);
        Ok(())
    }
}

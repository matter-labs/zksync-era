use std::collections::{BTreeMap, HashMap};

use zksync_basic_types::H256;

pub(crate) use self::patch::{PartialPatchSet, TreeUpdate};
use crate::{
    errors::{DeserializeContext, DeserializeError, DeserializeErrorKind},
    types::{KeyLookup, Leaf, Manifest, Node, NodeKey, Root},
};

mod patch;
#[cfg(test)]
mod tests;

/// Generic database functionality. Its main implementation is [`RocksDB`].
pub trait Database: Send + Sync {
    fn indices(&self, version: u64, keys: &[H256]) -> Result<Vec<KeyLookup>, DeserializeError>;

    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError>;

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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
struct InsertedKeyEntry {
    index: u64,
    inserted_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PatchSet {
    manifest: Manifest,
    patches_by_version: HashMap<u64, PartialPatchSet>,
    // We maintain a joint index for all versions to make it easier to use `PatchSet` as a `Database` or in a `Patched` wrapper.
    sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    // TODO: stale keys
}

impl PatchSet {
    fn index(&self, version: u64, key: &H256) -> KeyLookup {
        let (next_key, next_entry) = self
            .sorted_new_leaves
            .range(key..)
            .find(|(_, entry)| entry.inserted_at <= version)
            .expect("guards must be inserted into a tree on initialization");
        if next_key == key {
            return KeyLookup::Existing(next_entry.index);
        }

        let (prev_key, prev_entry) = self
            .sorted_new_leaves
            .range(..key)
            .rev()
            .find(|(_, entry)| entry.inserted_at <= version)
            .expect("guards must be inserted into a tree on initialization");
        KeyLookup::Missing {
            prev_key_and_index: (*prev_key, prev_entry.index),
            next_key_and_index: (*next_key, next_entry.index),
        }
    }
}

impl Database for PatchSet {
    fn indices(&self, version: u64, keys: &[H256]) -> Result<Vec<KeyLookup>, DeserializeError> {
        use rayon::prelude::*;

        let mut lookup = vec![];
        keys.par_iter()
            .map(|key| self.index(version, key))
            .collect_into_vec(&mut lookup);
        Ok(lookup)
    }

    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        Ok(Some(self.manifest.clone()))
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
        anyhow::ensure!(
            patch.manifest.version_count >= self.manifest.version_count,
            "truncating versions is not supported"
        );

        self.manifest = patch.manifest;
        self.patches_by_version.extend(patch.patches_by_version);
        self.sorted_new_leaves.extend(patch.sorted_new_leaves);
        Ok(())
    }
}

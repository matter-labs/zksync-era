use std::{
    cmp,
    collections::{BTreeMap, HashMap},
    ops,
};

use zksync_basic_types::H256;

pub(crate) use self::patch::{TreeUpdate, WorkingPatchSet};
pub use self::rocksdb::{MerkleTreeColumnFamily, RocksDBWrapper};
use crate::{
    errors::{DeserializeContext, DeserializeError, DeserializeErrorKind},
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root},
};

mod patch;
mod rocksdb;
mod serialization;
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

    /// Truncates the tree.
    fn truncate(
        &mut self,
        manifest: Manifest,
        truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()>;
}

impl<DB: Database + ?Sized> Database for &mut DB {
    fn indices(&self, version: u64, keys: &[H256]) -> Result<Vec<KeyLookup>, DeserializeError> {
        (**self).indices(version, keys)
    }

    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        (**self).try_manifest()
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        (**self).try_root(version)
    }

    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError> {
        (**self).try_nodes(keys)
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        (**self).apply_patch(patch)
    }

    fn truncate(
        &mut self,
        manifest: Manifest,
        truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()> {
        (**self).truncate(manifest, truncated_versions)
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
struct InsertedKeyEntry {
    index: u64,
    inserted_at: u64,
}

#[derive(Debug, Clone)]
struct PartialPatchSet {
    leaf_count: u64,
    // TODO: maybe, a wrapper around `Vec<(_, _)>` would be more efficient?
    /// `internal[0]` corresponds to a root, `internal[1]` to single-nibble nodes etc.
    internal: Vec<HashMap<u64, InternalNode>>,
    /// Sorted by the index.
    leaves: HashMap<u64, Leaf>,
}

impl PartialPatchSet {
    fn root(&self) -> Root {
        Root {
            leaf_count: self.leaf_count,
            root_node: self.internal[0][&0].clone(),
        }
    }

    fn node(&self, nibble_count: u8, index_on_level: u64) -> Option<Node> {
        let nibble_count = usize::from(nibble_count);
        let leaf_nibbles = self.internal.len();
        Some(match nibble_count.cmp(&leaf_nibbles) {
            cmp::Ordering::Less => {
                let level = &self.internal[nibble_count];
                level.get(&index_on_level)?.clone().into()
            }
            cmp::Ordering::Equal => (*self.leaves.get(&index_on_level)?).into(),
            cmp::Ordering::Greater => return None,
        })
    }
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

    #[cfg(test)]
    pub(crate) fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
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
        // We consider manifest absent if there are no tree versions. This is important for tree tag checks on the empty tree.
        Ok(if self.manifest.version_count == 0 {
            None
        } else {
            Some(self.manifest.clone())
        })
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        Ok(self
            .patches_by_version
            .get(&version)
            .map(PartialPatchSet::root))
    }

    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError> {
        let nodes = keys.iter().map(|key| {
            assert!(key.nibble_count > 0);

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
        let new_version_count = patch.manifest.version_count;
        anyhow::ensure!(
            new_version_count >= self.manifest.version_count,
            "Use `truncate()` for truncating tree"
        );

        self.manifest = patch.manifest;
        self.patches_by_version.extend(patch.patches_by_version);
        self.sorted_new_leaves.extend(patch.sorted_new_leaves);
        Ok(())
    }

    fn truncate(
        &mut self,
        manifest: Manifest,
        _truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()> {
        let new_version_count = manifest.version_count;
        self.patches_by_version
            .retain(|&version, _| version < new_version_count);
        // This requires a full scan, but we assume there aren't that many data in a patch (it's mostly used as a `Database` for testing).
        self.sorted_new_leaves
            .retain(|_, entry| entry.inserted_at < new_version_count);

        self.manifest = manifest;
        Ok(())
    }
}

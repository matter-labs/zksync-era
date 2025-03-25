use std::{
    cmp,
    collections::{BTreeMap, HashMap},
    ops, slice,
};

use zksync_basic_types::H256;

pub(crate) use self::patch::{TreeUpdate, WorkingPatchSet};
pub use self::rocksdb::{MerkleTreeColumnFamily, RocksDBWrapper};
use crate::{
    errors::{DeserializeContext, DeserializeError, DeserializeErrorKind},
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root, TreeEntry},
};

mod patch;
mod rocksdb;
mod serialization;
#[cfg(test)]
mod tests;

/// [`TreeEntry`] generalization allowing to check leaf index assignment when loading data from the tree.
pub(crate) trait AsEntry {
    fn as_entry(&self) -> &TreeEntry;

    fn check_index(&self, index: u64) -> anyhow::Result<()>;
}

impl AsEntry for TreeEntry {
    fn as_entry(&self) -> &TreeEntry {
        self
    }

    fn check_index(&self, _index: u64) -> anyhow::Result<()> {
        Ok(())
    }
}

impl AsEntry for (u64, TreeEntry) {
    fn as_entry(&self) -> &TreeEntry {
        &self.1
    }

    fn check_index(&self, index: u64) -> anyhow::Result<()> {
        let (ref_index, entry) = self;
        anyhow::ensure!(
            index == *ref_index,
            "Unexpected index for {entry:?}: reference is {ref_index}, but tree implies {index}",
        );
        Ok(())
    }
}

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

    /// Truncates the tree. `manifest` specifies the new number of tree versions, and `truncated_versions`
    /// contains the last version *before* the truncation. This operation should be atomic.
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

    fn total_internal_nodes(&self) -> usize {
        self.internal.iter().map(HashMap::len).sum()
    }
}

/// Immutable in-memory changeset that can atomically applied to a [`Database`].
#[derive(Debug, Clone, Default)]
pub struct PatchSet {
    manifest: Manifest,
    patches_by_version: HashMap<u64, PartialPatchSet>,
    // We maintain a joint index for all versions to make it easier to use `PatchSet` as a `Database` or in a `Patched` wrapper.
    sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    // TODO: stale keys
}

impl PatchSet {
    fn is_new_version(&self, version: u64) -> bool {
        version >= self.manifest.version_count // this patch truncates `version`
            || self.patches_by_version.contains_key(&version)
    }

    fn index(&self, version: u64, key: &H256) -> KeyLookup {
        let (next_key, next_idx) = self
            .sorted_new_leaves
            .range(key..)
            .find(|(_, entry)| entry.inserted_at <= version)
            .map(|(key, entry)| (*key, entry.index))
            // Default to the max guard even if it's not present in the patch set. This is important
            // for using `PatchSet` inside `Patched`.
            .unwrap_or_else(|| (H256::repeat_byte(0xff), 1));
        if next_key == *key {
            return KeyLookup::Existing(next_idx);
        }

        let (prev_key, prev_idx) = self
            .sorted_new_leaves
            .range(..key)
            .rev()
            .find(|(_, entry)| entry.inserted_at <= version)
            .map(|(key, entry)| (*key, entry.index))
            .unwrap_or_else(|| (H256::zero(), 0));
        KeyLookup::Missing {
            prev_key_and_index: (prev_key, prev_idx),
            next_key_and_index: (next_key, next_idx),
        }
    }

    fn copied_hashes_count(&self) -> usize {
        let copied_hashes = self.patches_by_version.iter().map(|(&version, patch)| {
            let copied_hashes = patch
                .internal
                .iter()
                .flat_map(HashMap::values)
                .map(|node| node.children.iter().filter(|r| r.version < version).count());
            copied_hashes.sum::<usize>()
        });
        copied_hashes.sum()
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

#[derive(Debug)]
pub struct Patched<DB> {
    inner: DB,
    patch: Option<PatchSet>,
}

impl<DB: Database> Patched<DB> {
    pub fn new(inner: DB) -> Self {
        Self { inner, patch: None }
    }

    /// Returns `Ok(None)` if the patch is not responsible for the requested version.
    fn lookup_patch(&self, key: &NodeKey) -> Result<Option<Node>, DeserializeError> {
        let Some(patch) = &self.patch else {
            return Ok(None);
        };

        Ok(if patch.is_new_version(key.version) {
            patch.try_nodes(slice::from_ref(key))?.into_iter().next()
        } else {
            None
        })
    }

    /// Flushes changes from RAM to the wrapped database.
    ///
    /// # Errors
    ///
    /// Proxies database I/O errors.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        if let Some(patch) = self.patch.take() {
            self.inner.apply_patch(patch)?;
        }
        Ok(())
    }

    /// Forgets about changes held in RAM.
    pub fn reset(&mut self) {
        self.patch = None;
    }

    /// Returns a reference to the underlying database. It is unsound to modify the database using this reference.
    pub fn inner(&self) -> &DB {
        &self.inner
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
    fn indices(&self, version: u64, keys: &[H256]) -> Result<Vec<KeyLookup>, DeserializeError> {
        let Some(patch) = &self.patch else {
            return self.inner.indices(version, keys);
        };
        if !patch.is_new_version(version) {
            return self.inner.indices(version, keys);
        }

        let patch_lookup = patch.indices(version, keys)?;
        let db_keys = keys.iter().zip(&patch_lookup).filter_map(|(key, lookup)| {
            if matches!(lookup, KeyLookup::Existing(_)) {
                // The key is present in the patch; not necessary to request it from the DB.
                None
            } else {
                Some(*key)
            }
        });
        let db_keys: Vec<_> = db_keys.collect();
        let mut db_lookup = self.inner.indices(version, &db_keys)?.into_iter();

        let merged = patch_lookup.into_iter().map(|from_patch| match from_patch {
            lookup @ KeyLookup::Existing(_) => lookup,
            KeyLookup::Missing {
                prev_key_and_index: patch_prev,
                next_key_and_index: patch_next,
            } => {
                match db_lookup.next().unwrap() {
                    // Exact match in the database, no need to merge
                    lookup @ KeyLookup::Existing(_) => lookup,
                    KeyLookup::Missing {
                        prev_key_and_index: db_prev,
                        next_key_and_index: db_next,
                    } => KeyLookup::Missing {
                        // Using `min` / `max` work because `(prev|next)_key_and_index` have key as the first element.
                        prev_key_and_index: patch_prev.max(db_prev),
                        next_key_and_index: patch_next.min(db_next),
                    },
                }
            }
        });
        Ok(merged.collect())
    }

    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        if let Some(patch) = &self.patch {
            Ok(Some(patch.manifest.clone()))
        } else {
            self.inner.try_manifest()
        }
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        if let Some(patch) = &self.patch {
            if patch.is_new_version(version) {
                return patch.try_root(version);
            }
        }
        self.inner.try_root(version)
    }

    fn try_nodes(&self, keys: &[NodeKey]) -> Result<Vec<Node>, DeserializeError> {
        if self.patch.is_none() {
            return self.inner.try_nodes(keys);
        }

        let mut is_in_patch = vec![false; keys.len()];
        let mut patch_nodes = vec![];
        for (key, is_in_patch) in keys.iter().zip(&mut is_in_patch) {
            if let Some(patch_node) = self.lookup_patch(key)? {
                *is_in_patch = true;
                patch_nodes.push(patch_node);
            }
        }

        let db_keys: Vec<_> = keys
            .iter()
            .zip(&is_in_patch)
            .filter_map(|(key, is_in_patch)| (!is_in_patch).then_some(*key))
            .collect();
        let mut db_nodes = self.inner.try_nodes(&db_keys)?.into_iter();
        let mut patch_nodes = patch_nodes.into_iter();

        let all_nodes = is_in_patch.into_iter().map(|is_in_patch| {
            if is_in_patch {
                patch_nodes.next().unwrap()
            } else {
                db_nodes.next().unwrap()
            }
        });
        Ok(all_nodes.collect())
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        if let Some(existing_patch) = &mut self.patch {
            existing_patch.apply_patch(patch)?;
        } else {
            self.patch = Some(patch);
        }
        Ok(())
    }

    fn truncate(
        &mut self,
        manifest: Manifest,
        truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()> {
        let Some(patch) = &mut self.patch else {
            return self.inner.truncate(manifest, truncated_versions);
        };
        patch.truncate(manifest.clone(), truncated_versions)?;

        // If the patch set is fully truncated, we want to flush truncation to the underlying DB since it won't be applied
        // in the following flushes.
        if patch.patches_by_version.is_empty() {
            assert!(patch.sorted_new_leaves.is_empty());

            self.patch = None;
            self.inner.truncate(manifest, truncated_versions)?;
        }
        Ok(())
    }
}

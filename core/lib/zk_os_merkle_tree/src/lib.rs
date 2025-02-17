//! Persistent ZK OS Merkle tree.

use std::{fmt, time::Instant};

use anyhow::Context as _;
use zksync_basic_types::H256;
pub use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

pub use self::{
    errors::DeserializeError,
    hasher::HashTree,
    storage::{Database, MerkleTreeColumnFamily, PatchSet, RocksDBWrapper},
    types::{BatchOutput, TreeEntry},
};
use crate::{
    storage::{TreeUpdate, WorkingPatchSet},
    types::MAX_TREE_DEPTH,
};

mod consistency;
mod errors;
mod hasher;
mod storage;
#[cfg(test)]
mod tests;
mod types;

/// Marker trait for tree parameters.
pub trait TreeParams: fmt::Debug + Send + Sync {
    type Hasher: HashTree;

    const TREE_DEPTH: u8;
    const INTERNAL_NODE_DEPTH: u8;
}

#[inline(always)]
pub(crate) fn leaf_nibbles<P: TreeParams>() -> u8 {
    P::TREE_DEPTH.div_ceil(P::INTERNAL_NODE_DEPTH)
}

#[inline(always)]
pub(crate) const fn max_nibbles_for_internal_node<P: TreeParams>() -> u8 {
    P::TREE_DEPTH.div_ceil(P::INTERNAL_NODE_DEPTH) - 1
}

#[inline(always)]
pub(crate) const fn max_node_children<P: TreeParams>() -> u8 {
    1 << P::INTERNAL_NODE_DEPTH
}

#[derive(Debug)]
pub struct DefaultTreeParams<const TREE_DEPTH: u8 = 64, const INTERNAL_NODE_DEPTH: u8 = 4>(());

impl<const TREE_DEPTH: u8, const INTERNAL_NODE_DEPTH: u8> TreeParams
    for DefaultTreeParams<TREE_DEPTH, INTERNAL_NODE_DEPTH>
{
    type Hasher = Blake2Hasher;
    const TREE_DEPTH: u8 = {
        assert!(TREE_DEPTH > 0 && TREE_DEPTH <= MAX_TREE_DEPTH);
        TREE_DEPTH
    };
    const INTERNAL_NODE_DEPTH: u8 = {
        assert!(INTERNAL_NODE_DEPTH > 0 && INTERNAL_NODE_DEPTH < 8); // to fit child count into `u8`
        INTERNAL_NODE_DEPTH
    };
}

#[derive(Debug)]
pub struct MerkleTree<DB, P: TreeParams = DefaultTreeParams> {
    db: DB,
    hasher: P::Hasher,
}

impl<DB: Database> MerkleTree<DB> {
    /// Loads a tree with the default Blake2 hasher.
    ///
    /// # Errors
    ///
    /// Errors in the same situations as [`Self::with_hasher()`].
    pub fn new(db: DB) -> anyhow::Result<Self> {
        Self::with_hasher(db, Blake2Hasher)
    }
}

impl<DB: Database, P: TreeParams> MerkleTree<DB, P> {
    /// Loads a tree with the specified hasher.
    ///
    /// # Errors
    ///
    /// Errors if the hasher or basic tree parameters (e.g., the tree depth)
    /// do not match those of the tree loaded from the database.
    pub fn with_hasher(db: DB, hasher: P::Hasher) -> anyhow::Result<Self> {
        let maybe_manifest = db.try_manifest().context("failed reading tree manifest")?;
        if let Some(manifest) = maybe_manifest {
            manifest.tags.ensure_consistency::<P>(&hasher)?;
        }
        Ok(Self { db, hasher })
    }

    /// Returns the root hash of a tree at the specified `version`, or `None` if the version
    /// was not written yet.
    pub fn root_hash(&self, version: u64) -> anyhow::Result<Option<H256>> {
        let Some(root) = self.db.try_root(version)? else {
            return Ok(None);
        };
        Ok(Some(root.hash::<P>(&self.hasher)))
    }

    /// Returns the latest version of the tree present in the database, or `None` if
    /// no versions are present yet.
    pub fn latest_version(&self) -> anyhow::Result<Option<u64>> {
        let Some(manifest) = self.db.try_manifest()? else {
            return Ok(None);
        };
        Ok(manifest.version_count.checked_sub(1))
    }

    pub fn latest_root_hash(&self) -> anyhow::Result<Option<H256>> {
        let Some(version) = self
            .latest_version()
            .context("failed getting latest version")?
        else {
            return Ok(None);
        };
        self.root_hash(version)
    }

    /// Extends this tree by creating its new version.
    ///
    /// All keys in the provided entries must be distinct.
    ///
    /// # Return value
    ///
    /// Returns information about the update such as the final tree hash.
    ///
    /// # Errors
    ///
    /// Proxies database I/O errors.
    #[tracing::instrument(level = "debug", skip_all, fields(latest_version))]
    pub fn extend(&mut self, entries: &[TreeEntry]) -> anyhow::Result<BatchOutput> {
        let latest_version = self
            .latest_version()
            .context("failed getting latest version")?;
        tracing::Span::current().record("latest_version", latest_version);

        let started_at = Instant::now();
        let (mut patch, update) = if let Some(version) = latest_version {
            self.create_patch(version, entries)
                .context("failed loading tree data")?
        } else {
            (
                WorkingPatchSet::<P>::empty(),
                TreeUpdate::for_empty_tree(entries)?,
            )
        };
        tracing::debug!(
            elapsed = ?started_at.elapsed(),
            inserts = update.inserts.len(),
            updates = update.updates.len(),
            loaded_internal_nodes = patch.total_internal_nodes(),
            "loaded tree data"
        );

        let started_at = Instant::now();
        let update = patch.update(update);
        tracing::debug!(elapsed = ?started_at.elapsed(), "updated tree structure");

        let started_at = Instant::now();
        let (patch, root_hash) = patch.finalize(&self.hasher, update);
        tracing::debug!(elapsed = ?started_at.elapsed(), "hashed tree");

        let started_at = Instant::now();
        self.db
            .apply_patch(patch)
            .context("failed persisting tree changes")?;
        tracing::debug!(elapsed = ?started_at.elapsed(), "persisted tree");
        Ok(BatchOutput { root_hash })
    }

    pub fn truncate_recent_versions(&mut self, retained_version_count: u64) -> anyhow::Result<()> {
        let mut manifest = self.db.try_manifest()?.unwrap_or_default();
        let current_version_count = manifest.version_count;
        if current_version_count > retained_version_count {
            // TODO: It is necessary to remove "future" stale keys since otherwise they may be used in future pruning and lead
            //   to non-obsolete tree nodes getting removed.
            manifest.version_count = retained_version_count;
            self.db.truncate(manifest, ..current_version_count)?;
        }
        Ok(())
    }
}

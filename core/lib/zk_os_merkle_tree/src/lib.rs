//! Persistent ZK OS Merkle tree.

use std::fmt;

use anyhow::Context as _;
use zksync_basic_types::H256;
pub use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

pub use self::{
    errors::DeserializeError,
    hasher::{BatchTreeProof, HashTree, TreeOperation},
    reader::MerkleTreeReader,
    storage::{Database, MerkleTreeColumnFamily, PatchSet, Patched, RocksDBWrapper},
    types::{BatchOutput, TreeEntry},
};
use crate::{
    metrics::{BatchProofStage, LoadStage, MerkleTreeInfo, METRICS},
    storage::{AsEntry, TreeUpdate, WorkingPatchSet},
    types::{Leaf, MAX_TREE_DEPTH},
};

mod consistency;
mod errors;
mod hasher;
mod metrics;
mod reader;
mod storage;
#[cfg(test)]
mod tests;
mod types;

/// Unstable types that should not be used unless you know what you're doing (e.g., implementing
/// `Database` trait for a custom type). There are no guarantees whatsoever that APIs / structure of
/// these types will remain stable.
#[doc(hidden)]
pub mod unstable {
    pub use crate::types::{KeyLookup, Leaf, Manifest, Node, NodeKey, RawNode, Root};
}

/// Marker trait for tree parameters.
pub trait TreeParams: fmt::Debug + Send + Sync {
    /// Hasher used by the tree.
    type Hasher: HashTree;

    /// Total tree depth. E.g., 64 means that a tree can accommodate up to `1 << 64` leaves (including guards).
    const TREE_DEPTH: u8;
    /// Depth of internal nodes in the tree. E.g., 3 means that an internal node has up to `1 << 3 = 8` children
    /// (i.e., radix-8 amortization). This parameter is an implementation detail; for an outside observer (e.g., for proofs),
    /// the tree is always binary.
    const INTERNAL_NODE_DEPTH: u8;
}

/// Returns the number of nibbles in [`NodeKey`](types::NodeKey)s for leaves.
#[inline(always)]
pub(crate) const fn leaf_nibbles<P: TreeParams>() -> u8 {
    P::TREE_DEPTH.div_ceil(P::INTERNAL_NODE_DEPTH)
}

/// Returns the max number of nibbles  in [`NodeKey`](types::NodeKey)s for internal nodes.
#[inline(always)]
pub(crate) const fn max_nibbles_for_internal_node<P: TreeParams>() -> u8 {
    P::TREE_DEPTH.div_ceil(P::INTERNAL_NODE_DEPTH) - 1
}

#[inline(always)]
pub(crate) const fn max_node_children<P: TreeParams>() -> u8 {
    1 << P::INTERNAL_NODE_DEPTH
}

/// Default Merkle tree parameters that should balance its performance and I/O requirements.
#[derive(Debug)]
pub struct DefaultTreeParams<const TREE_DEPTH: u8 = 64, const INTERNAL_NODE_DEPTH: u8 = 3>(());

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

impl<DB: Database, P: TreeParams> MerkleTree<DB, P>
where
    P::Hasher: Default,
{
    /// Returns the hash of the empty tree.
    pub fn empty_tree_hash() -> H256 {
        let hasher = P::Hasher::default();
        let min_guard_hash = hasher.hash_leaf(&Leaf::MIN_GUARD);
        let max_guard_hash = hasher.hash_leaf(&Leaf::MAX_GUARD);
        let mut hash = hasher.hash_branch(&min_guard_hash, &max_guard_hash);
        for depth in 1..P::TREE_DEPTH {
            hash = hasher.hash_branch(&hash, &hasher.empty_subtree_hash(depth));
        }
        hash
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
        if let Some(manifest) = &maybe_manifest {
            manifest.tags.ensure_consistency::<P>(&hasher)?;
        }

        let info = MerkleTreeInfo {
            hasher: hasher.name(),
            depth: P::TREE_DEPTH.into(),
            internal_node_depth: P::INTERNAL_NODE_DEPTH.into(),
        };
        tracing::debug!(?info, manifest = ?maybe_manifest, "initialized Merkle tree");
        METRICS.info.set(info).ok();

        Ok(Self { db, hasher })
    }

    /// Returns a reference to the database.
    pub fn db(&self) -> &DB {
        &self.db
    }

    /// Returns the root hash of a tree at the specified `version`, or `None` if the version
    /// was not written yet.
    pub fn root_hash(&self, version: u64) -> anyhow::Result<Option<H256>> {
        let Some(root) = self.db.try_root(version)? else {
            return Ok(None);
        };
        Ok(Some(root.hash::<P>(&self.hasher)))
    }

    /// Returns the root hash and leaf count at the specified version.
    pub fn root_info(&self, version: u64) -> anyhow::Result<Option<(H256, u64)>> {
        let Some(root) = self.db.try_root(version)? else {
            return Ok(None);
        };
        let root_hash = root.hash::<P>(&self.hasher);
        Ok(Some((root_hash, root.leaf_count)))
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

    /// Creates a batch proof for `keys` at the specified tree version.
    ///
    /// # Errors
    ///
    /// - Returns an error if the version doesn't exist.
    /// - Proxies database errors.
    pub fn prove(&self, version: u64, keys: &[H256]) -> anyhow::Result<BatchTreeProof> {
        let (patch, mut update) = self.create_patch::<TreeEntry>(version, &[], keys)?;
        Ok(patch.create_batch_proof(&self.hasher, vec![], update.take_read_operations()))
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
    pub fn extend(&mut self, entries: &[TreeEntry]) -> anyhow::Result<BatchOutput> {
        let (output, _) = self.extend_inner(entries, None)?;
        Ok(output)
    }

    /// Same as [`Self::extend()`], but with a cross-check for the leaf indices of `entries`.
    ///
    /// `reference_indices` must have the same length as `entries`. Note that because indices are *only* used for cross-checking,
    /// entries (more specifically, new insertions) must still be correctly ordered!
    pub fn extend_with_reference(
        &mut self,
        entries: &[(u64, TreeEntry)],
    ) -> anyhow::Result<BatchOutput> {
        let (output, _) = self.extend_inner(entries, None)?;
        Ok(output)
    }

    #[tracing::instrument(level = "debug", name = "extend", skip_all, fields(latest_version))]
    fn extend_inner(
        &mut self,
        entries: &[impl AsEntry],
        read_keys: Option<&[H256]>,
    ) -> anyhow::Result<(BatchOutput, Option<BatchTreeProof>)> {
        let latest_version = self
            .latest_version()
            .context("failed getting latest version")?;
        tracing::Span::current().record("latest_version", latest_version);

        let load_nodes_latency = METRICS.load_nodes_latency[&LoadStage::Total].start();
        let (mut patch, mut update) = if let Some(version) = latest_version {
            self.create_patch(version, entries, read_keys.unwrap_or_default())
                .context("failed loading tree data")?
        } else {
            (
                WorkingPatchSet::<P>::empty(),
                TreeUpdate::for_empty_tree(entries)?,
            )
        };
        let elapsed = load_nodes_latency.observe();
        METRICS.batch_inserts_count.observe(update.inserts.len());
        METRICS.batch_updates_count.observe(update.updates.len());
        METRICS.loaded_leaves.observe(patch.loaded_leaves_count());
        METRICS
            .loaded_internal_nodes
            .observe(patch.loaded_internal_nodes_count());
        if let Some(keys) = read_keys {
            METRICS
                .batch_reads_count
                .observe(keys.len() - update.missing_reads_count);
            METRICS
                .batch_missing_reads_count
                .observe(update.missing_reads_count);
        }

        tracing::debug!(
            ?elapsed,
            inserts = update.inserts.len(),
            updates = update.updates.len(),
            reads = read_keys.map(|keys| keys.len() - update.missing_reads_count),
            missing_reads = read_keys.is_some().then_some(update.missing_reads_count),
            loaded_internal_nodes = patch.loaded_internal_nodes_count(),
            "loaded tree data"
        );

        let proof = if read_keys.is_some() {
            let proof_latency = METRICS.batch_proof_latency[&BatchProofStage::Total].start();
            let proof = patch.create_batch_proof(
                &self.hasher,
                update.take_operations(),
                update.take_read_operations(),
            );
            let elapsed = proof_latency.observe();
            METRICS.proof_hashes_count.observe(proof.hashes.len());

            tracing::debug!(
                ?elapsed,
                proof.leaves.len = proof.sorted_leaves.len(),
                proof.hashes.len = proof.hashes.len(),
                "created batch proof"
            );
            Some(proof)
        } else {
            None
        };

        let extend_patch_latency = METRICS.extend_patch_latency.start();
        let update = patch.update(update);
        let elapsed = extend_patch_latency.observe();
        tracing::debug!(?elapsed, "updated tree structure");

        let finalize_latency = METRICS.finalize_patch_latency.start();
        let (patch, output) = patch.finalize(&self.hasher, update);
        let elapsed = finalize_latency.observe();
        tracing::debug!(?elapsed, "hashed tree");

        let apply_patch_latency = METRICS.apply_patch_latency.start();
        self.db
            .apply_patch(patch)
            .context("failed persisting tree changes")?;
        let elapsed = apply_patch_latency.observe();
        tracing::debug!(?elapsed, "persisted tree");

        METRICS.leaf_count.set(output.leaf_count);
        Ok((output, proof))
    }

    /// Same as [Self::extend()`], but also provides a Merkle proof of the update.
    pub fn extend_with_proof(
        &mut self,
        entries: &[TreeEntry],
        read_keys: &[H256],
    ) -> anyhow::Result<(BatchOutput, BatchTreeProof)> {
        let (output, proof) = self.extend_inner(entries, Some(read_keys))?;
        Ok((output, proof.unwrap()))
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

impl<DB: Database, P: TreeParams> MerkleTree<Patched<DB>, P> {
    /// Flushes changes to the underlying storage.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.db.flush()
    }
}

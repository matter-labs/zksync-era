//! Merkle tree recovery logic.
//!
//! # Overview
//!
//! **Recovery process** is responsible for restoring a Merkle tree from a snapshot. A snapshot
//! consists of all tree entries at a specific tree version. As a result of recovery, we create
//! a Merkle tree with the same entries as the snapshot. Any changes that are applied to the tree
//! afterwards will have the same outcome as if they were applied to the original tree.
//!
//! Importantly, a recovered tree is only *observably* identical to the original tree; it differs
//! in (currently un-observable) node versions. In a recovered tree, all nodes will initially have
//! the same version (the snapshot version), while in the original tree, node versions are distributed
//! from 0 to the snapshot version (both inclusive).
//!
//! Recovery process proceeds as follows:
//!
//! 1. Initialize a tree in the recovery mode. Until recovery is finished, the tree cannot be accessed
//!    using ordinary [`MerkleTree`] APIs.
//! 2. Update the tree from a snapshot, which [is fed to the tree](MerkleTreeRecovery::extend())
//!    as [`RecoveryEntry`] chunks. Recovery entries must be ordered by increasing key.
//! 3. Finalize recovery using [`MerkleTreeRecovery::finalize()`]. To check integrity, you may compare
//!    [`MerkleTreeRecovery::root_hash()`] to the reference value.
//!
//! The recovery process is tolerant to crashes and may be resumed from the middle. To find the latest
//! recovered key, you may use [`MerkleTreeRecovery::last_processed_key()`].
//!
//! `RecoveryEntry` chunks are not validated during recovery. They can be authenticated using
//! [`TreeRangeDigest`](crate::TreeRangeDigest)s provided that the tree root hash is authenticated
//! using external means.
//!
//! # Implementation details
//!
//! We require `RecoveryEntry` ordering to simplify tracking the recovery progress. It also makes
//! node updates more efficient. Indeed, it suffices to load a leaf with the greatest key and its ancestors
//! before extending the tree; these nodes are guaranteed to be the *only* DB reads necessary
//! to insert new entries.

use std::{collections::HashMap, time::Instant};

use anyhow::Context as _;
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

pub use crate::storage::PersistenceThreadHandle;
use crate::{
    hasher::{HashTree, HasherWithStats},
    metrics::{RecoveryStage, RECOVERY_METRICS},
    storage::{Database, MaybeParallel, PatchSet, PruneDatabase, PrunePatchSet, Storage},
    types::{Key, Manifest, Root, TreeEntry, TreeTags, ValueHash},
};

#[cfg(test)]
mod tests;

/// Handle to a Merkle tree during its recovery.
#[derive(Debug)]
pub struct MerkleTreeRecovery<DB, H = Blake2Hasher> {
    pub(crate) db: MaybeParallel<DB>,
    hasher: H,
    recovered_version: u64,
}

impl<DB: PruneDatabase> MerkleTreeRecovery<DB> {
    /// Creates tree recovery with the default Blake2 hasher.
    ///
    /// # Errors
    ///
    /// Errors in the same situations as [`Self::with_hasher()`].
    pub fn new(db: DB, recovered_version: u64) -> anyhow::Result<Self> {
        Self::with_hasher(db, recovered_version, Blake2Hasher)
    }
}

impl<DB: PruneDatabase, H: HashTree> MerkleTreeRecovery<DB, H> {
    /// Loads a tree with the specified hasher.
    ///
    /// # Errors
    ///
    /// - Errors if the tree DB exists and it's not being recovered, or if it's being recovered
    ///   for a different tree version.
    /// - Errors if the hasher or basic tree parameters (e.g., the tree depth)
    ///   do not match those of the tree loaded from the database.
    pub fn with_hasher(mut db: DB, recovered_version: u64, hasher: H) -> anyhow::Result<Self> {
        let manifest = db.manifest();
        let mut manifest = if let Some(manifest) = manifest {
            if manifest.version_count > 0 {
                let expected_version = manifest.version_count - 1;
                anyhow::ensure!(
                    recovered_version == expected_version,
                    "Requested to recover tree version {recovered_version}, but it is currently being recovered \
                    for version {expected_version}"
                );
            }
            manifest
        } else {
            Manifest {
                version_count: recovered_version + 1,
                tags: None,
            }
        };

        manifest.version_count = recovered_version + 1;
        if let Some(tags) = &manifest.tags {
            tags.ensure_consistency(&hasher, true)?;
        } else {
            let mut tags = TreeTags::new(&hasher);
            tags.is_recovering = true;
            manifest.tags = Some(tags);
        }
        db.apply_patch(PatchSet::from_manifest(manifest))?;

        Ok(Self {
            db: MaybeParallel::Sequential(db),
            hasher,
            recovered_version,
        })
    }

    /// Updates custom tags for the tree using the provided closure. The update is atomic and unconditional.
    ///
    /// # Errors
    ///
    /// Propagates database I/O errors.
    pub fn update_custom_tags<R>(
        &mut self,
        update: impl FnOnce(&mut HashMap<String, String>) -> R,
    ) -> anyhow::Result<R> {
        let mut manifest = self
            .db
            .manifest()
            .context("Merkle tree manifest disappeared")?;
        let tags = manifest
            .tags
            .get_or_insert_with(|| TreeTags::new(&self.hasher));
        let output = update(&mut tags.custom);
        self.db.apply_patch(PatchSet::from_manifest(manifest))?;
        Ok(output)
    }

    /// Returns the version of the tree being recovered.
    pub fn recovered_version(&self) -> u64 {
        self.recovered_version
    }

    /// Returns the root hash of the recovered tree at this point.
    pub fn root_hash(&self) -> ValueHash {
        let root = self.db.root(self.recovered_version);
        let Some(Root::Filled { node, .. }) = root else {
            return self.hasher.empty_tree_hash();
        };
        node.hash(&mut HasherWithStats::new(&self.hasher), 0)
    }

    /// Returns the last key processed during the recovery process.
    pub fn last_processed_key(&self) -> Option<Key> {
        let storage = Storage::new(&self.db, &self.hasher, self.recovered_version, false);
        storage.greatest_key()
    }

    /// Extends a tree with a chunk of linearly ordered entries.
    ///
    /// Entries must be ordered by increasing `key`, and the key of the first entry must be greater
    /// than [`Self::last_processed_key()`].
    ///
    /// # Panics
    ///
    /// Panics if entry keys are not correctly ordered.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            recovered_version = self.recovered_version,
            entries.len = entries.len(),
            %entries.key_range = entries_key_range(&entries),
        ),
    )]
    #[allow(clippy::missing_errors_doc)]
    pub fn extend_linear(&mut self, entries: Vec<TreeEntry>) -> anyhow::Result<()> {
        tracing::debug!("Started extending tree");
        RECOVERY_METRICS.chunk_size.observe(entries.len());

        let stage_latency = RECOVERY_METRICS.stage_latency[&RecoveryStage::Extend].start();
        let storage = Storage::new(&self.db, &self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_linear_recovery(entries);
        let stage_latency = stage_latency.observe();
        tracing::debug!("Finished processing keys; took {stage_latency:?}");

        let stage_latency = RECOVERY_METRICS.stage_latency[&RecoveryStage::ApplyPatch].start();
        self.db.apply_patch(patch)?;
        let stage_latency = stage_latency.observe();
        tracing::debug!("Finished persisting to DB; took {stage_latency:?}");
        Ok(())
    }

    /// Extends a tree with a chunk of entries. Unlike [`Self::extend_linear()`], entries may be
    /// ordered in any way you like.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            recovered_version = self.recovered_version,
            entries.len = entries.len(),
        ),
    )]
    #[allow(clippy::missing_errors_doc)]
    pub fn extend_random(&mut self, entries: Vec<TreeEntry>) -> anyhow::Result<()> {
        tracing::debug!("Started extending tree");
        RECOVERY_METRICS.chunk_size.observe(entries.len());

        let stage_latency = RECOVERY_METRICS.stage_latency[&RecoveryStage::Extend].start();
        let storage = Storage::new(&self.db, &self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_random_recovery(entries);
        let stage_latency = stage_latency.observe();
        tracing::debug!("Finished processing keys; took {stage_latency:?}");

        let stage_latency = RECOVERY_METRICS.stage_latency[&RecoveryStage::ApplyPatch].start();
        self.db.apply_patch(patch)?;
        let stage_latency = stage_latency.observe();
        tracing::debug!("Finished persisting to DB; took {stage_latency:?}");
        Ok(())
    }

    /// Finalizes the recovery process marking it as complete in the tree manifest.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(recovered_version = self.recovered_version),
    )]
    #[allow(
        clippy::missing_panics_doc,
        clippy::range_plus_one,
        clippy::missing_errors_doc
    )]
    pub fn finalize(mut self) -> anyhow::Result<DB> {
        let mut manifest = self.db.manifest().unwrap();
        // ^ `unwrap()` is safe: manifest is inserted into the DB on creation

        let leaf_count = if let Some(root) = self.db.root(self.recovered_version) {
            root.leaf_count()
        } else {
            // Marginal case: an empty tree is recovered (i.e., `extend()` was never called).
            let patch = PatchSet::for_empty_root(manifest.clone(), self.recovered_version);
            self.db.apply_patch(patch)?;
            0
        };
        tracing::debug!(
            "Finalizing recovery of the Merkle tree with {leaf_count} key–value entries"
        );

        let started_at = Instant::now();
        let stale_keys = self.db.stale_keys(self.recovered_version);
        let stale_keys_len = stale_keys.len();
        tracing::debug!("Pruning {stale_keys_len} accumulated stale keys");
        let prune_patch = PrunePatchSet::new(
            stale_keys,
            self.recovered_version..self.recovered_version + 1,
        );
        self.db.prune(prune_patch)?;
        tracing::debug!(
            "Pruned {stale_keys_len} stale keys in {:?}",
            started_at.elapsed()
        );

        manifest
            .tags
            .get_or_insert_with(|| TreeTags::new(&self.hasher))
            .is_recovering = false;
        self.db.apply_patch(PatchSet::from_manifest(manifest))?;
        tracing::debug!("Updated tree manifest to mark recovery as complete");

        self.db.join()
    }
}

impl<DB: 'static + Clone + PruneDatabase, H: HashTree> MerkleTreeRecovery<DB, H> {
    /// Offloads database persistence to a background thread, so that it can run at the same time as processing of the following chunks.
    /// Chunks are still guaranteed to be persisted atomically and in order.
    ///
    /// # Arguments
    ///
    /// - `buffer_capacity` determines how many chunks can be buffered before persistence blocks (i.e., back-pressure).
    ///   Also controls memory usage, since each chunk translates into a non-trivial database patch (order of 1 kB / entry;
    ///   i.e., a chunk with 200,000 entries would translate to a 200 MB patch).
    ///
    /// # Return value
    ///
    /// On success, returns a handle allowing to control background persistence thread. For now, it can only be used to emulate persistence crashes;
    /// the handle can be dropped otherwise.
    ///
    /// # Safety
    ///
    /// If recovery is interrupted (e.g., its process crashes), then some of the latest chunks may not be persisted,
    /// and will need to be processed again. It is **unsound** to restart recovery while a persistence thread may be active;
    /// this may lead to a corrupted database state.
    ///
    /// # Errors
    ///
    /// Returns an error if `buffer_capacity` is 0, or if persistence was already parallelized.
    pub fn parallelize_persistence(
        &mut self,
        buffer_capacity: usize,
    ) -> anyhow::Result<PersistenceThreadHandle> {
        anyhow::ensure!(buffer_capacity > 0, "Buffer capacity must be positive");
        self.db
            .parallelize(self.recovered_version, buffer_capacity)
            .context("persistence is already parallelized")
    }

    /// Waits until all changes in the underlying database are persisted, i.e. all chunks are flushed into it.
    /// This is only relevant if [persistence was parallelized](Self::parallelize_persistence()) earlier;
    /// otherwise, this method will return immediately.
    ///
    /// # Errors
    ///
    /// Propagates database I/O errors, should they occur during persistence.
    pub fn wait_for_persistence(self) -> anyhow::Result<()> {
        self.db.join()?;
        Ok(())
    }
}

fn entries_key_range(entries: &[TreeEntry]) -> String {
    let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
        return "(empty)".to_owned();
    };
    format!("{:0>64x}..={:0>64x}", first.key, last.key)
}

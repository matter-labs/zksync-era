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
//! in (currently unobservable) node versions. In a recovered tree, all nodes will initially have
//! the same version (the snapshot version), while in the original tree, node versions are distributed
//! from 0 to the snapshot version (both inclusive).
//!
//! Recovery process proceeds as follows:
//!
//! 1. Initialize a tree in the recovery mode. Until recovery is finished, the tree cannot be accessed
//!   using ordinary [`MerkleTree`] APIs.
//! 2. Update the tree from a snapshot, which [is fed to the tree](MerkleTreeRecovery::extend())
//!   as [`RecoveryEntry`] chunks. Recovery entries must be ordered by increasing key.
//! 3. Finalize recovery using [`MerkleTreeRecovery::finalize()`]. To check integrity, you may compare
//!   [`MerkleTreeRecovery::root_hash()`] to the reference value.
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

use std::time::Instant;

use crate::{
    hasher::{HashTree, HasherWithStats},
    storage::{PatchSet, PruneDatabase, PrunePatchSet, Storage},
    types::{Key, Manifest, Root, TreeTags, ValueHash},
    MerkleTree,
};
use zksync_crypto::hasher::blake2::Blake2Hasher;

/// Entry in a Merkle tree used during recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryEntry {
    /// Entry key.
    pub key: Key,
    /// Entry value.
    pub value: ValueHash,
    /// Leaf index associated with the entry. It is **not** checked whether leaf indices are well-formed
    /// during recovery (e.g., that they are unique).
    pub leaf_index: u64,
}

/// Handle to a Merkle tree during its recovery.
#[derive(Debug)]
pub struct MerkleTreeRecovery<DB, H = Blake2Hasher> {
    db: DB,
    hasher: H,
    recovered_version: u64,
}

impl<DB: PruneDatabase> MerkleTreeRecovery<DB> {
    /// Creates tree recovery with the default Blake2 hasher.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(db: DB, recovered_version: u64) -> Self {
        Self::with_hasher(db, recovered_version, Blake2Hasher)
    }
}

impl<DB: PruneDatabase, H: HashTree> MerkleTreeRecovery<DB, H> {
    /// Loads a tree with the specified hasher.
    ///
    /// # Panics
    ///
    /// - Panics if the tree DB exists and it's not being recovered, or if it's being recovered
    ///   for a different tree version.
    /// - Panics if the hasher or basic tree parameters (e.g., the tree depth)
    ///   do not match those of the tree loaded from the database.
    pub fn with_hasher(mut db: DB, recovered_version: u64, hasher: H) -> Self {
        let manifest = db.manifest();
        let mut manifest = if let Some(manifest) = manifest {
            if manifest.version_count > 0 {
                let expected_version = manifest.version_count - 1;
                assert_eq!(
                    recovered_version,
                    expected_version,
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
            tags.assert_consistency(&hasher, true);
        } else {
            let mut tags = TreeTags::new(&hasher);
            tags.is_recovering = true;
            manifest.tags = Some(tags);
        }
        db.apply_patch(PatchSet::from_manifest(manifest));

        Self {
            db,
            hasher,
            recovered_version,
        }
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
    pub fn extend_linear(&mut self, entries: Vec<RecoveryEntry>) {
        tracing::debug!("Started extending tree");

        let started_at = Instant::now();
        let storage = Storage::new(&self.db, &self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_linear_recovery(entries);
        tracing::debug!("Finished processing keys; took {:?}", started_at.elapsed());

        let started_at = Instant::now();
        self.db.apply_patch(patch);
        tracing::debug!("Finished persisting to DB; took {:?}", started_at.elapsed());
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
    pub fn extend_random(&mut self, entries: Vec<RecoveryEntry>) {
        tracing::debug!("Started extending tree");

        let started_at = Instant::now();
        let storage = Storage::new(&self.db, &self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_random_recovery(entries);
        tracing::debug!("Finished processing keys; took {:?}", started_at.elapsed());

        let started_at = Instant::now();
        self.db.apply_patch(patch);
        tracing::debug!("Finished persisting to DB; took {:?}", started_at.elapsed());
    }

    /// Finalizes the recovery process marking it as complete in the tree manifest.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(recovered_version = self.recovered_version),
    )]
    #[allow(clippy::missing_panics_doc, clippy::range_plus_one)]
    pub fn finalize(mut self) -> MerkleTree<DB, H> {
        let mut manifest = self.db.manifest().unwrap();
        // ^ `unwrap()` is safe: manifest is inserted into the DB on creation

        let leaf_count = if let Some(root) = self.db.root(self.recovered_version) {
            root.leaf_count()
        } else {
            // Marginal case: an empty tree is recovered (i.e., `extend()` was never called).
            let patch = PatchSet::for_empty_root(manifest.clone(), self.recovered_version);
            self.db.apply_patch(patch);
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
        self.db.prune(prune_patch);
        tracing::debug!(
            "Pruned {stale_keys_len} stale keys in {:?}",
            started_at.elapsed()
        );

        manifest
            .tags
            .get_or_insert_with(|| TreeTags::new(&self.hasher))
            .is_recovering = false;
        self.db.apply_patch(PatchSet::from_manifest(manifest));
        tracing::debug!("Updated tree manifest to mark recovery as complete");

        // We don't need additional integrity checks since they were performed in the constructor
        MerkleTree {
            db: self.db,
            hasher: self.hasher,
        }
    }
}

fn entries_key_range(entries: &[RecoveryEntry]) -> String {
    let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
        return "(empty)".to_owned();
    };
    format!("{:0>64x}..={:0>64x}", first.key, last.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hasher::HasherWithStats, types::LeafNode};

    #[test]
    #[should_panic(expected = "Tree is expected to be in the process of recovery")]
    fn recovery_for_initialized_tree() {
        let mut db = PatchSet::default();
        MerkleTreeRecovery::new(&mut db, 123).finalize();
        MerkleTreeRecovery::new(db, 123);
    }

    #[test]
    #[should_panic(expected = "Requested to recover tree version 42")]
    fn recovery_for_different_version() {
        let mut db = PatchSet::default();
        MerkleTreeRecovery::new(&mut db, 123);
        MerkleTreeRecovery::new(&mut db, 42);
    }

    #[test]
    fn recovering_empty_tree() {
        let tree = MerkleTreeRecovery::new(PatchSet::default(), 42).finalize();
        assert_eq!(tree.latest_version(), Some(42));
        assert_eq!(tree.root(42), Some(Root::Empty));
    }

    #[test]
    fn recovering_tree_with_single_node() {
        let mut recovery = MerkleTreeRecovery::new(PatchSet::default(), 42);
        let recovery_entry = RecoveryEntry {
            key: Key::from(123),
            value: ValueHash::repeat_byte(1),
            leaf_index: 1,
        };
        recovery.extend_linear(vec![recovery_entry]);
        let tree = recovery.finalize();

        assert_eq!(tree.latest_version(), Some(42));
        let mut hasher = HasherWithStats::new(&Blake2Hasher);
        assert_eq!(
            tree.latest_root_hash(),
            LeafNode::new(
                recovery_entry.key,
                recovery_entry.value,
                recovery_entry.leaf_index
            )
            .hash(&mut hasher, 0)
        );
        tree.verify_consistency(42).unwrap();
    }
}

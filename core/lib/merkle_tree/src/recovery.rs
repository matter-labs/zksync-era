//! Merkle tree recovery logic.

use crate::{
    hasher::HashTree,
    storage::{PatchSet, PruneDatabase, PrunePatchSet, Storage},
    types::{Key, Manifest, Root, TreeTags, ValueHash},
    MerkleTree,
};
use zksync_crypto::hasher::blake2::Blake2Hasher;

// FIXME: add logging

/// Entry in a Merkle tree used during recovery.
#[derive(Debug, Clone)]
pub struct RecoveryEntry {
    /// Entry key.
    pub key: Key,
    /// Entry value.
    pub value: ValueHash,
    /// Leaf index associated with the entry. It is **not** checked whether leaf indices are well-formed
    /// during recovery (e.g., that they are unique).
    pub leaf_index: u64,
    /// Version of this entry. It is **not** checked, whether this version makes sense together with
    /// `leaf_index`; it's only checked that it does not exceed the recovered tree version.
    pub version: u64,
}

/// Handle to a Merkle tree during its recovery.
#[derive(Debug)]
pub struct MerkleTreeRecovery<'a, DB> {
    db: DB,
    hasher: &'a dyn HashTree,
    recovered_version: u64,
}

impl<'a, DB: PruneDatabase> MerkleTreeRecovery<'a, DB> {
    /// Creates tree recovery with the default Blake2 hasher.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(db: DB, recovered_version: u64) -> Self {
        Self::with_hasher(db, recovered_version, &Blake2Hasher)
    }

    /// Loads a tree with the specified hasher.
    ///
    /// # Panics
    ///
    /// - Panics if the tree DB exists and it's not being recovered, or if it's being recovered
    ///   for a different tree version.
    /// - Panics if the hasher or basic tree parameters (e.g., the tree depth)
    ///   do not match those of the tree loaded from the database.
    pub fn with_hasher(mut db: DB, recovered_version: u64, hasher: &'a dyn HashTree) -> Self {
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
            tags.assert_consistency(hasher, true);
        } else {
            let mut tags = TreeTags::new(hasher);
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
        node.hash(&mut self.hasher.into(), 0)
    }

    /// Returns the last key processed during the recovery process.
    pub fn last_processed_key(&self) -> Option<Key> {
        let storage = Storage::new(&self.db, self.hasher, self.recovered_version, false);
        storage.greatest_key()
    }

    /// Extends a tree with a chunk of entries.
    ///
    /// Entries must be ordered by increasing `key`, and the key of the first entry must be greater
    /// than [`Self::last_processed_key()`].
    ///
    /// # Panics
    ///
    /// Panics if entry keys are not correctly ordered.
    pub fn extend(&mut self, recovery_entries: Vec<RecoveryEntry>) {
        let storage = Storage::new(&self.db, self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_recovery(recovery_entries);
        self.db.apply_patch(patch);
    }

    /// Finalizes the recovery process marking it as complete in the tree manifest.
    #[allow(clippy::missing_panics_doc, clippy::range_plus_one)]
    pub fn finalize(mut self) -> MerkleTree<'a, DB> {
        let mut manifest = self.db.manifest().unwrap();
        // ^ `unwrap()` is safe: manifest is inserted into the DB on creation
        manifest
            .tags
            .get_or_insert_with(|| TreeTags::new(self.hasher))
            .is_recovering = false;
        self.db.apply_patch(PatchSet::from_manifest(manifest));

        // Prune all accumulated stale keys.
        let stale_keys = self.db.stale_keys(self.recovered_version);
        let prune_patch = PrunePatchSet::new(
            stale_keys,
            self.recovered_version..self.recovered_version + 1,
        );
        self.db.prune(prune_patch);

        // We don't need additional checks since they were performed in the constructor
        MerkleTree {
            db: self.db,
            hasher: self.hasher,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

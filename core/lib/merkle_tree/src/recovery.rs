//! Merkle tree recovery logic.

#![allow(missing_docs)] // FIXME

use crate::{
    hasher::HashTree,
    storage::{Database, PatchSet, Storage},
    types::{Key, Manifest, Root, TreeTags, ValueHash},
    MerkleTree,
};
use zksync_crypto::hasher::blake2::Blake2Hasher;

#[derive(Debug, Clone)]
pub struct RecoveryEntry {
    pub key: Key,
    pub value: ValueHash,
    pub leaf_index: u64,
    // FIXME: add version
}

#[derive(Debug)]
pub struct MerkleTreeRecovery<'a, DB> {
    db: DB,
    hasher: &'a dyn HashTree,
    recovered_version: u64,
}

impl<'a, DB: Database> MerkleTreeRecovery<'a, DB> {
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
    /// FIXME
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
    pub fn extend(&mut self, recovery_entries: Vec<RecoveryEntry>) {
        let storage = Storage::new(&self.db, self.hasher, self.recovered_version, false);
        let patch = storage.extend_during_recovery(recovery_entries);
        dbg!(patch.manifest());
        self.db.apply_patch(patch);
    }

    /// Finalizes the recovery process marking it as complete in the tree manifest.
    #[allow(clippy::missing_panics_doc)]
    pub fn finalize(mut self) -> MerkleTree<'a, DB> {
        let mut manifest = self.db.manifest().unwrap();
        // ^ `unwrap()` is safe: manifest is inserted into the DB on creation
        manifest
            .tags
            .get_or_insert_with(|| TreeTags::new(self.hasher))
            .is_recovering = false;
        self.db.apply_patch(PatchSet::from_manifest(manifest));

        // We don't need additional checks since they were performed in the constructor
        MerkleTree {
            db: self.db,
            hasher: self.hasher,
        }
    }
}

// FIXME: cover panics with unit tests

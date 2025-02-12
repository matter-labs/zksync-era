//! Persistent ZK OS Merkle tree.

use anyhow::Context as _;
use zksync_basic_types::H256;
pub use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

pub use self::{errors::DeserializeError, hasher::HashTree, storage::Database, types::TreeEntry};
use crate::storage::{PartialPatchSet, TreeUpdate};

mod errors;
mod hasher;
mod storage;
mod types;

#[derive(Debug)]
pub struct MerkleTree<DB, H = Blake2Hasher> {
    db: DB,
    hasher: H,
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

impl<DB: Database, H: HashTree> MerkleTree<DB, H> {
    /// Loads a tree with the specified hasher.
    ///
    /// # Errors
    ///
    /// Errors if the hasher or basic tree parameters (e.g., the tree depth)
    /// do not match those of the tree loaded from the database.
    pub fn with_hasher(db: DB, hasher: H) -> anyhow::Result<Self> {
        todo!()
    }

    /// Returns the root hash of a tree at the specified `version`, or `None` if the version
    /// was not written yet.
    pub fn root_hash(&self, version: u64) -> Option<H256> {
        todo!()
    }

    /// Returns the latest version of the tree present in the database, or `None` if
    /// no versions are present yet.
    pub fn latest_version(&self) -> anyhow::Result<Option<u64>> {
        let Some(manifest) = self.db.try_manifest()? else {
            return Ok(None);
        };
        Ok(manifest.version_count.checked_sub(1))
    }

    /// Extends this tree by creating its new version.
    ///
    /// # Return value
    ///
    /// Returns information about the update such as the final tree hash.
    ///
    /// # Errors
    ///
    /// Proxies database I/O errors.
    pub fn extend(&mut self, entries: Vec<TreeEntry>) -> anyhow::Result<()> {
        let latest_version = self
            .latest_version()
            .context("failed getting latest version")?;
        let (mut patch, update) = if let Some(version) = latest_version {
            self.create_patch(version, &entries)
                .context("failed loading tree data")?
        } else {
            (
                PartialPatchSet::empty(),
                TreeUpdate::for_empty_tree(&entries),
            )
        };

        let update = patch.update(update);
        let patch = patch.finalize(&self.hasher, update);
        self.db
            .apply_patch(patch)
            .context("failed persisting tree changes")?;
        Ok(())
    }
}

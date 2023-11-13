//! Sparse Merkle tree implementation based on Diem [Jellyfish Merkle tree].
//!
//! # Overview
//!
//! The crate provides two major abstractions: domain-independent [`MerkleTree`] and
//! domain-specific [`ZkSyncTree`](domain::ZkSyncTree); the latter wraps `MerkleTree`.
//!
//! The database backend is abstracted via the [`Database`] trait (a key-value storage), which has
//! the following implementations:
//!
//! - [`RocksDBWrapper`] is a wrapper around RocksDB
//! - [`PatchSet`] is an in-memory implementation useful for testing / benchmarking
//! - [`Patched`] is a wrapper combining the persistent backend and a [`PatchSet`]. It's used
//!   in `ZkSyncTree` to accumulate changes before flushing them to RocksDB.
//!
//! The hashing backend is abstracted via the [`HashTree`] trait, which has the following
//! implementations:
//!
//! - [`Blake2Hasher`] is the main implementation based on Blake2s-256
//! - `()` provides a no-op implementation useful for benchmarking.
//!
//! # Tree hashing specification
//!
//! A tree is hashed as if it was a full binary Merkle tree with `2^256` leaves:
//!
//! - Hash of a vacant leaf is `hash([0_u8; 40])`, where `hash` is the hash function used
//!   (Blake2s-256).
//! - Hash of an occupied leaf is `hash(u64::to_be_bytes(leaf_index) ++ value_hash)`,
//!   where `leaf_index` is the 1-based index of the leaf key in the order of insertion,
//!   `++` is byte concatenation.
//! - Hash of an internal node is `hash(left_child_hash ++ right_child_hash)`.
//!
//! [Jellyfish Merkle tree]: https://developers.diem.com/papers/jellyfish-merkle-tree/2021-01-14.pdf

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::doc_markdown // frequent false positive: RocksDB
)]

mod consistency;
pub mod domain;
mod errors;
mod getters;
mod hasher;
mod metrics;
mod pruning;
pub mod recovery;
mod storage;
mod types;
mod utils;

/// Unstable types that should not be used unless you know what you're doing (e.g., implementing
/// `Database` trait for a custom type). There are no guarantees whatsoever that APIs / structure of
/// these types will remain stable.
#[doc(hidden)]
pub mod unstable {
    pub use crate::{
        errors::DeserializeError,
        types::{Manifest, Node, NodeKey, Root},
    };
}

pub use crate::{
    errors::NoVersionError,
    hasher::{HashTree, TreeRangeDigest},
    pruning::{MerkleTreePruner, MerkleTreePrunerHandle},
    storage::{
        Database, MerkleTreeColumnFamily, PatchSet, Patched, PruneDatabase, PrunePatchSet,
        RocksDBWrapper,
    },
    types::{
        BlockOutput, BlockOutputWithProofs, Key, TreeEntry, TreeEntryWithProof, TreeInstruction,
        TreeLogEntry, TreeLogEntryWithProof, ValueHash,
    },
};

use crate::{hasher::HasherWithStats, storage::Storage, types::Root};
use zksync_crypto::hasher::blake2::Blake2Hasher;

/// Binary Merkle tree implemented using AR16MT from Diem [Jellyfish Merkle tree] white paper.
///
/// A tree is persistent and is backed by a key-value store (the `DB` type param). It is versioned,
/// meaning that the store retains *all* versions of the tree since its inception. A version
/// corresponds to a block number in the domain model; it is a `u64` counter incremented each time
/// a block of changes is committed into the tree via [`Self::extend()`]. It is possible to reset
/// the tree to a previous version via [`Self::truncate_versions()`].
///
/// # Tree structure
///
/// The tree store principally contains the following information:
///
/// - The tree *manifest* specifying version-independent information (right now, this is just
///   the number of versions).
/// - For each of the stored versions: tree *root* containing the number of leaves
///   and the root node of the tree.
/// - *Nodes* of the particular version of the tree keyed by version + the path from the root
///   of the tree to the node.
///
/// To be more I/O-efficient (at the cost of some additional hashing operations), the tree
/// is stored in the radix-16 format. That is, each internal node may have up to 16 children.
/// From the storage perspective, an internal node contains *child references*. A reference
/// consists of the following data:
///
/// - Version of the tree the child first appeared in
/// - Node type (internal node or leaf; used for deserialization)
/// - Node hash
///
/// Tree nodes are immutable; that's why addressing a child by version works, and a hash
/// mentioned in a child reference cannot become outdated. Immutability and structuring storage
/// keys for tree nodes so that nodes of the same version are grouped together makes
/// DB read / write patterns optimal for RocksDB.
///
/// Another optimization is that paths of internal nodes that do not fork (i.e., lead to
/// a single child) are removed. In other words, a leaf node may be placed at any tree level,
/// not just the lowest possible one. Correspondingly, a leaf node besides a value hash
/// stores the full key, since it cannot be restored from other information.
///
/// The I/O optimizations do not influence tree hashing.
///
/// [Jellyfish Merkle tree]: https://developers.diem.com/papers/jellyfish-merkle-tree/2021-01-14.pdf
#[derive(Debug)]
pub struct MerkleTree<DB, H = Blake2Hasher> {
    db: DB,
    hasher: H,
}

impl<DB: Database> MerkleTree<DB> {
    /// Loads a tree with the default Blake2 hasher.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(db: DB) -> Self {
        Self::with_hasher(db, Blake2Hasher)
    }
}

impl<DB: Database, H: HashTree> MerkleTree<DB, H> {
    /// Loads a tree with the specified hasher.
    ///
    /// # Panics
    ///
    /// Panics if the hasher or basic tree parameters (e.g., the tree depth)
    /// do not match those of the tree loaded from the database.
    pub fn with_hasher(db: DB, hasher: H) -> Self {
        let tags = db.manifest().and_then(|manifest| manifest.tags);
        if let Some(tags) = tags {
            tags.assert_consistency(&hasher, false);
        }
        // If there are currently no tags in the tree, we consider that it fits
        // for backward compatibility. The tags will be added the next time the tree is saved.

        Self { db, hasher }
    }

    /// Returns the root hash of a tree at the specified `version`, or `None` if the version
    /// was not written yet.
    pub fn root_hash(&self, version: u64) -> Option<ValueHash> {
        let root = self.root(version)?;
        let Root::Filled { node, .. } = root else {
            return Some(self.hasher.empty_tree_hash());
        };
        Some(node.hash(&mut HasherWithStats::new(&self.hasher), 0))
    }

    pub(crate) fn root(&self, version: u64) -> Option<Root> {
        self.db.root(version)
    }

    /// Returns the latest version of the tree present in the database, or `None` if
    /// no versions are present yet.
    pub fn latest_version(&self) -> Option<u64> {
        self.db.manifest()?.version_count.checked_sub(1)
    }

    /// Returns the root hash for the latest version of the tree.
    pub fn latest_root_hash(&self) -> ValueHash {
        let root_hash = self
            .latest_version()
            .and_then(|version| self.root_hash(version));
        root_hash.unwrap_or_else(|| self.hasher.empty_tree_hash())
    }

    /// Returns the latest-versioned root node.
    pub(crate) fn latest_root(&self) -> Root {
        let root = self.latest_version().and_then(|version| self.root(version));
        root.unwrap_or(Root::Empty)
    }

    /// Removes the most recent versions from the database.
    ///
    /// The current implementation does not actually remove node data for the removed versions
    /// since it's likely to be reused in the future (especially upper-level internal nodes).
    pub fn truncate_recent_versions(&mut self, retained_version_count: u64) {
        let mut manifest = self.db.manifest().unwrap_or_default();
        if manifest.version_count > retained_version_count {
            manifest.version_count = retained_version_count;
            let patch = PatchSet::from_manifest(manifest);
            self.db.apply_patch(patch);
        }
    }

    /// Extends this tree by creating its new version.
    ///
    /// # Return value
    ///
    /// Returns information about the update such as the final tree hash.
    pub fn extend(&mut self, key_value_pairs: Vec<(Key, ValueHash)>) -> BlockOutput {
        let next_version = self.db.manifest().unwrap_or_default().version_count;
        let storage = Storage::new(&self.db, &self.hasher, next_version, true);
        let (output, patch) = storage.extend(key_value_pairs);
        self.db.apply_patch(patch);
        output
    }

    /// Extends this tree by creating its new version, computing an authenticity Merkle proof
    /// for each provided instruction.
    ///
    /// # Return value
    ///
    /// Returns information about the update such as the final tree hash and proofs for each input
    /// instruction.
    pub fn extend_with_proofs(
        &mut self,
        instructions: Vec<(Key, TreeInstruction)>,
    ) -> BlockOutputWithProofs {
        let next_version = self.db.manifest().unwrap_or_default().version_count;
        let storage = Storage::new(&self.db, &self.hasher, next_version, true);
        let (output, patch) = storage.extend_with_proofs(instructions);
        self.db.apply_patch(patch);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TreeTags;

    #[test]
    #[should_panic(expected = "Unsupported tree architecture `AR64MT`, expected `AR16MT`")]
    fn tree_architecture_mismatch() {
        let mut db = PatchSet::default();
        db.manifest_mut().tags = Some(TreeTags {
            architecture: "AR64MT".to_owned(),
            depth: 256,
            hasher: "blake2s256".to_string(),
            is_recovering: false,
        });

        MerkleTree::new(db);
    }

    #[test]
    #[should_panic(expected = "Unexpected tree depth: expected 256, got 128")]
    fn tree_depth_mismatch() {
        let mut db = PatchSet::default();
        db.manifest_mut().tags = Some(TreeTags {
            architecture: "AR16MT".to_owned(),
            depth: 128,
            hasher: "blake2s256".to_string(),
            is_recovering: false,
        });

        MerkleTree::new(db);
    }

    #[test]
    #[should_panic(expected = "Mismatch between the provided tree hasher `blake2s256`")]
    fn hasher_mismatch() {
        let mut db = PatchSet::default();
        db.manifest_mut().tags = Some(TreeTags {
            architecture: "AR16MT".to_owned(),
            depth: 256,
            hasher: "sha256".to_string(),
            is_recovering: false,
        });

        MerkleTree::new(db);
    }
}

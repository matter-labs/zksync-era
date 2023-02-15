//! In-database Merkle Tree implementation.

#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use thiserror::Error;
use zksync_crypto::hasher::Hasher;
use zksync_types::U256;

mod iter_ext;
mod patch;
mod storage;
#[cfg(test)]
mod tests;
mod tree_config;
mod types;
mod utils;
mod zksync_tree;

use types::Bytes;
pub use types::{InitialStorageWrite, RepeatedStorageWrite, TreeMetadata};
pub use zksync_tree::{TreeMode, ZkSyncTree};

/// All kinds of Merkle Tree errors.
#[derive(Error, Clone, Debug)]
pub enum TreeError {
    #[error("Branch entry with given level and hash was not found: {0:?} {1:?}")]
    MissingBranch(u16, Vec<u8>),
    #[error("Leaf entry with given hash was not found: {0:?}")]
    MissingLeaf(Vec<u8>),
    #[error("Key shouldn't be greater than {0:?}, received {1:?}")]
    InvalidKey(U256, U256),
    #[error("Failed to convert {0:?} to `U256`")]
    KeyConversionFailed(String),
    #[error("Invalid depth for {0:?}: {1:?} != {2:?}")]
    InvalidDepth(String, u16, u16),
    #[error("Attempt to create read-only Merkle tree for the absent root")]
    EmptyRoot,
    #[error("Invalid root: {0:?}")]
    InvalidRoot(Vec<u8>),
    #[error("Trees have different roots: {0:?} and {1:?} respectively")]
    TreeRootsDiffer(Vec<u8>, Vec<u8>),
    #[error("storage access error")]
    StorageIoError(#[from] zksync_storage::rocksdb::Error),
    #[error("empty patch")]
    EmptyPatch,
}

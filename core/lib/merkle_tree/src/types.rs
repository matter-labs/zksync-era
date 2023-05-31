//! Definitions of types used in Merkle Tree implementation.

use crate::U256;
use serde::Serialize;
use std::collections::HashMap;
use zksync_crypto::hasher::blake2::Blake2Hasher;
pub use zksync_types::writes::{InitialStorageWrite, RepeatedStorageWrite};
use zksync_types::{proofs::PrepareBasicCircuitsJob, H256};
use zksync_utils::impl_from_wrapper;

/// Position of a node in a tree.
/// The first argument (u16) is the depth, and the second (u256) is the index on this depth.
/// So the root, will have (0, 0)
/// Its children: (1,0), (1,1)
/// Their children: (2, 0), (2,1), (2,2), (2,3) etc.
#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize)]
pub struct LevelIndex(pub (u16, U256));

impl_from_wrapper!(LevelIndex, (u16, U256));

impl LevelIndex {
    pub fn bin_key(&self) -> Vec<u8> {
        bincode::serialize(&self).expect("Serialization failed")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeOperation {
    Write {
        value: TreeValue,
        previous_value: TreeValue,
    },
    Read(TreeValue),
    Delete,
}

#[derive(Clone, Debug)]
pub enum NodeEntry {
    Branch {
        hash: Vec<u8>,
        left_hash: Vec<u8>,
        right_hash: Vec<u8>,
    },
    Leaf {
        hash: Vec<u8>,
    },
}

impl NodeEntry {
    pub fn hash(&self) -> &[u8] {
        match self {
            NodeEntry::Branch { hash, .. } => hash,
            NodeEntry::Leaf { hash } => hash,
        }
    }

    pub fn into_hash(self) -> Vec<u8> {
        match self {
            NodeEntry::Branch { hash, .. } => hash,
            NodeEntry::Leaf { hash } => hash,
        }
    }
}

/// Convenience aliases to make code a bit more readable.
pub type TreeKey = U256;
pub type TreeValue = H256;
pub type Bytes = Vec<u8>;

/// Definition of the main hashing scheme to be used throughout the module.
/// We use an alias instead of direct type definition for the case if we'd decide to switch the hashing scheme
pub type ZkHasher = Blake2Hasher;

/// Definition of the hash type derived from the hasher.
pub type ZkHash = Bytes;

/// Represents metadata of current tree state
/// Includes root hash, current tree location and serialized merkle paths for each storage log
#[derive(Debug, Clone, Default)]
pub struct TreeMetadata {
    pub root_hash: H256,
    pub rollup_last_leaf_index: u64,
    pub witness_input: Option<PrepareBasicCircuitsJob>,
    pub initial_writes: Vec<InitialStorageWrite>,
    pub repeated_writes: Vec<RepeatedStorageWrite>,
}

#[derive(Debug, Clone, Default)]
pub struct LeafIndices {
    pub leaf_indices: HashMap<TreeKey, u64>,
    pub last_index: u64,
    pub previous_index: u64,
    pub initial_writes: Vec<InitialStorageWrite>,
    pub repeated_writes: Vec<RepeatedStorageWrite>,
}

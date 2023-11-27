//! Basic storage types.

mod internal;

pub(crate) use self::internal::{
    ChildRef, Nibbles, NibblesBytes, StaleNodeKey, TreeTags, HASH_SIZE, KEY_SIZE, TREE_DEPTH,
};
pub use self::internal::{InternalNode, LeafNode, Manifest, Node, NodeKey, Root};

use zksync_types::{H256, U256};

/// Key stored in the tree.
pub type Key = U256;
/// Hash type of values and intermediate nodes in the tree.
pub type ValueHash = H256;

/// Instruction to read or write a tree value at a certain key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeInstruction {
    /// Read the current tree value.
    Read,
    /// Write the specified value.
    Write(TreeEntry),
}

/// Entry in a Merkle tree associated with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeEntry {
    /// Value associated with the key.
    pub value_hash: ValueHash,
    /// Enumeration index of the key.
    pub leaf_index: u64,
}

impl From<LeafNode> for TreeEntry {
    fn from(leaf: LeafNode) -> Self {
        Self {
            value_hash: leaf.value_hash,
            leaf_index: leaf.leaf_index,
        }
    }
}

impl TreeEntry {
    /// Creates a new entry with the specified fields.
    pub fn new(leaf_index: u64, value_hash: ValueHash) -> Self {
        Self {
            value_hash,
            leaf_index,
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            value_hash: ValueHash::zero(),
            leaf_index: 0,
        }
    }

    /// Returns `true` iff this entry encodes lack of a value.
    pub fn is_empty(&self) -> bool {
        self.leaf_index == 0 && self.value_hash.is_zero()
    }

    pub(crate) fn with_merkle_path(self, merkle_path: Vec<ValueHash>) -> TreeEntryWithProof {
        TreeEntryWithProof {
            base: self,
            merkle_path,
        }
    }

    /// Replaces the value in this entry and returns the modified entry.
    #[must_use]
    pub fn with_value(self, value_hash: H256) -> Self {
        Self { value_hash, ..self }
    }
}

/// Entry in a Merkle tree together with a proof of authenticity.
#[derive(Debug, Clone)]
pub struct TreeEntryWithProof {
    /// Entry in a Merkle tree.
    pub base: TreeEntry,
    /// Proof of the value authenticity.
    ///
    /// If specified, a proof is the Merkle path consisting of up to 256 hashes
    /// ordered starting the bottommost level of the tree (one with leaves) and ending before
    /// the root level.
    ///
    /// If the path is not full (contains <256 hashes), it means that the hashes at the beginning
    /// corresponding to the empty subtrees are skipped. This allows compacting the proof ~10x.
    pub merkle_path: Vec<ValueHash>,
}

/// Output of inserting a block of entries into a Merkle tree.
#[derive(Debug, PartialEq, Eq)]
pub struct BlockOutput {
    /// The new hash of the tree.
    pub root_hash: ValueHash,
    /// The number of leaves in the tree after the update.
    pub leaf_count: u64,
    /// Information about each insertion / update operation in the order of application.
    pub logs: Vec<TreeLogEntry>,
}

/// Information about an the effect of a [`TreeInstruction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeLogEntry {
    /// A node was inserted into the tree.
    Inserted,
    /// A node with the specified index was updated.
    Updated(TreeEntry),
    /// A node was read from the tree.
    Read(TreeEntry),
    /// A missing key was read.
    ReadMissingKey,
}

impl TreeLogEntry {
    pub(crate) fn is_read(&self) -> bool {
        matches!(self, Self::Read { .. } | Self::ReadMissingKey)
    }
}

/// Extended output of inserting a block of entries into a Merkle tree that contains
/// Merkle proofs for each operation.
#[derive(Debug)]
pub struct BlockOutputWithProofs {
    /// Extended information about each insertion / update operation in the order of application.
    pub logs: Vec<TreeLogEntryWithProof>,
    /// The number of leaves in the tree after the update.
    pub leaf_count: u64,
}

impl BlockOutputWithProofs {
    /// Returns the final root hash of the Merkle tree.
    pub fn root_hash(&self) -> Option<ValueHash> {
        Some(self.logs.last()?.root_hash)
    }
}

/// [`TreeLogEntry`] together with its authenticity proof.
#[derive(Debug)]
pub struct TreeLogEntryWithProof<P = Vec<ValueHash>> {
    /// Log entry about an atomic operation on the tree.
    pub base: TreeLogEntry,
    /// Merkle path to prove log authenticity. The path consists of up to 256 hashes
    /// ordered starting the bottommost level of the tree (one with leaves) and ending before
    /// the root level.
    ///
    /// If the path is not full (contains <256 hashes), it means that the hashes at the beginning
    /// corresponding to the empty subtrees are skipped. This allows compacting the proof ~10x.
    pub merkle_path: P,
    /// Root tree hash after the operation.
    pub root_hash: ValueHash,
}

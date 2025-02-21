use std::fmt;

use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

use crate::{DefaultTreeParams, HashTree, TreeParams};

/// Maximum supported tree depth (to fit indexes into `u64`).
pub(crate) const MAX_TREE_DEPTH: u8 = 64;

/// Tree leaf.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Leaf {
    pub key: H256,
    pub value: H256,
    /// 0-based index of a leaf with the lexicographically previous key.
    pub prev_index: u64,
    /// 0-based index of a leaf with the lexicographically next key.
    pub next_index: u64,
}

impl Leaf {
    /// Minimum guard leaf inserted at the tree at its initialization.
    pub const MIN_GUARD: Self = Self {
        key: H256::zero(),
        value: H256::zero(),
        prev_index: 0,
        next_index: 1,
    };

    /// Maximum guard leaf inserted at the tree at its initialization.
    pub const MAX_GUARD: Self = Self {
        key: H256::repeat_byte(0xff),
        value: H256::zero(),
        prev_index: 0,
        next_index: 1,
    };
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ChildRef {
    pub(crate) version: u64,
    pub(crate) hash: H256,
}

/// Internal node of the tree, potentially amortized to have higher number of child references
/// (e.g., 8 or 16 instead of 2), depending on [`TreeParams`].
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct InternalNode {
    pub(crate) children: Vec<ChildRef>,
}

impl InternalNode {
    pub(crate) fn empty() -> Self {
        Self { children: vec![] }
    }

    pub(crate) fn new(len: usize, version: u64) -> Self {
        Self {
            children: vec![
                ChildRef {
                    version,
                    hash: H256::zero()
                };
                len
            ],
        }
    }

    /// Panics if the index doesn't exist.
    pub(crate) fn child_ref(&self, index: usize) -> &ChildRef {
        &self.children[index]
    }

    pub(crate) fn child_mut(&mut self, index: usize) -> &mut ChildRef {
        &mut self.children[index]
    }

    pub(crate) fn ensure_len(&mut self, expected_len: usize, version: u64) {
        self.children.resize_with(expected_len, || ChildRef {
            version,
            hash: H256::zero(),
        });
    }
}

/// Arbitrary tree node.
#[derive(Debug, Clone)]
pub enum Node {
    Internal(InternalNode),
    Leaf(Leaf),
}

impl From<InternalNode> for Node {
    fn from(node: InternalNode) -> Self {
        Self::Internal(node)
    }
}

impl From<Leaf> for Node {
    fn from(leaf: Leaf) -> Self {
        Self::Leaf(leaf)
    }
}

/// Result of a key lookup in the tree.
///
/// Either a leaf with this key is already present in the tree, or there are neighbor leaves, which need to be updated during insertion
/// or included into the proof for missing reads.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum KeyLookup {
    Existing(u64),
    Missing {
        prev_key_and_index: (H256, u64),
        next_key_and_index: (H256, u64),
    },
}

/// Unique key for a versioned tree node.
#[derive(Clone, Copy)]
pub struct NodeKey {
    /// Tree version.
    pub(crate) version: u64,
    /// 0 is root, 1 is its children etc.
    pub(crate) nibble_count: u8,
    pub(crate) index_on_level: u64,
}

impl NodeKey {
    pub(crate) const fn root(version: u64) -> Self {
        Self {
            version,
            nibble_count: 0,
            index_on_level: 0,
        }
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}nibs:{}",
            self.version, self.nibble_count, self.index_on_level
        )
    }
}

impl fmt::Debug for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Tree root: a node + additional metadata (for now, just the number of leaves in the tree).
#[derive(Debug, Clone)]
pub struct Root {
    pub(crate) leaf_count: u64,
    pub(crate) root_node: InternalNode,
}

/// Entry in a Merkle tree associated with a key. Provided as an input for [`MerkleTree`](crate::MerkleTree) operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeEntry {
    /// Tree key.
    pub key: H256,
    /// Value associated with the key.
    pub value: H256,
}

impl TreeEntry {
    pub(crate) const MIN_GUARD: Self = Self {
        key: H256::zero(),
        value: H256::zero(),
    };

    pub(crate) const MAX_GUARD: Self = Self {
        key: H256::repeat_byte(0xff),
        value: H256::zero(),
    };
}

/// Persisted tags associated with a tree.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct TreeTags {
    pub architecture: String,
    pub depth: u8,
    pub internal_node_depth: u8,
    pub hasher: String,
}

impl Default for TreeTags {
    fn default() -> Self {
        Self::for_params::<DefaultTreeParams>(&Blake2Hasher)
    }
}

impl TreeTags {
    const ARCHITECTURE: &'static str = "AmortizedLinkedListMT";

    pub(crate) fn for_params<P: TreeParams>(hasher: &P::Hasher) -> Self {
        Self {
            architecture: Self::ARCHITECTURE.to_owned(),
            depth: P::TREE_DEPTH,
            internal_node_depth: P::INTERNAL_NODE_DEPTH,
            hasher: hasher.name().to_owned(),
        }
    }

    pub(crate) fn ensure_consistency<P: TreeParams>(
        &self,
        hasher: &P::Hasher,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.architecture == Self::ARCHITECTURE,
            "Unsupported tree architecture `{}`, expected `{}`",
            self.architecture,
            Self::ARCHITECTURE
        );
        anyhow::ensure!(
            self.depth == P::TREE_DEPTH,
            "Unexpected tree depth: expected {expected}, got {got}",
            expected = P::TREE_DEPTH,
            got = self.depth
        );
        anyhow::ensure!(
            self.internal_node_depth == P::INTERNAL_NODE_DEPTH,
            "Unexpected internal node depth: expected {expected}, got {got}",
            expected = P::INTERNAL_NODE_DEPTH,
            got = self.internal_node_depth
        );
        anyhow::ensure!(
            hasher.name() == self.hasher,
            "Mismatch between the provided tree hasher `{}` and the hasher `{}` used \
             in the database",
            hasher.name(),
            self.hasher
        );
        Ok(())
    }
}

/// Version-independent information about the tree.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    /// Number of tree versions stored in the database.
    pub(crate) version_count: u64,
    pub(crate) tags: TreeTags,
}

/// Output of updating / inserting data in a [`MerkleTree`](crate::MerkleTree).
#[derive(Debug, Clone, Copy)]
pub struct BatchOutput {
    /// New root hash of the tree.
    pub root_hash: H256,
    /// New leaf count (including 2 guard entries).
    pub leaf_count: u64,
}

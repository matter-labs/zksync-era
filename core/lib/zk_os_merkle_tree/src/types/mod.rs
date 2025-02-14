use std::fmt;

use zksync_basic_types::H256;

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

    /// Number of nibbles for leaves, i.e. tree depth (64) divided by the levels per nibble (4).
    pub(crate) const NIBBLES: u8 = 16;
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ChildRef {
    pub(crate) version: u64,
    pub(crate) hash: H256,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct InternalNode {
    pub(crate) children: Vec<ChildRef>,
    // TODO: hashing cache?
}

impl InternalNode {
    /// Maximum number of nibbles for internal nodes.
    pub(crate) const MAX_NIBBLES: u8 = Leaf::NIBBLES - 1;

    pub(crate) fn empty() -> Self {
        Self { children: vec![] }
    }

    pub(crate) fn new(len: usize, version: u64) -> Self {
        assert!(len <= 16);
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
        assert!(index < 16);
        &self.children[index]
    }

    pub(crate) fn child_mut(&mut self, index: usize) -> &mut ChildRef {
        assert!(index < 16);
        &mut self.children[index]
    }

    pub(crate) fn ensure_len(&mut self, expected_len: usize, version: u64) {
        assert!(expected_len <= 16);
        self.children.resize_with(expected_len, || ChildRef {
            version,
            hash: H256::zero(),
        });
    }
}

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

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum KeyLookup {
    Existing(u64),
    Missing {
        prev_key_and_index: (H256, u64),
        next_key_and_index: (H256, u64),
    },
}

#[derive(Clone, Copy)]
pub struct NodeKey {
    pub(crate) version: u64,
    /// `0..=16` (64 / 4), where 0 is the root and 16 are leaves.
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

    pub(crate) fn is_leaf(&self) -> bool {
        self.nibble_count == Leaf::NIBBLES
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.nibble_count <= InternalNode::MAX_NIBBLES {
            write!(
                formatter,
                "{}:[{} @ {} nibs]",
                self.version, self.index_on_level, self.nibble_count
            )
        } else {
            write!(formatter, "{}:{}", self.version, self.index_on_level)
        }
    }
}

impl fmt::Debug for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

#[derive(Debug, Clone)]
pub struct Root {
    pub(crate) leaf_count: u64,
    pub(crate) root_node: InternalNode,
}

/// Entry in a Merkle tree associated with a key.
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

/// Version-independent information about the tree.
#[derive(Debug, Default, Clone)]
pub struct Manifest {
    // Number of tree versions stored in the database.
    pub(crate) version_count: u64,
}

//! Basic storage types.

use std::{fmt, num::NonZeroU64};

use zksync_types::{H256, U256};

use crate::{
    hasher::{HashTree, InternalNodeCache},
    utils::SmallMap,
};

/// Size of a (leaf) tree key in bytes.
pub(crate) const KEY_SIZE: usize = 32;
/// Depth of the tree (= number of bits in `KEY_SIZE`).
pub(crate) const TREE_DEPTH: usize = KEY_SIZE * 8;
/// Size of a hashed value in bytes.
pub(crate) const HASH_SIZE: usize = 32;

/// Instruction to read or write a tree value at a certain key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeInstruction {
    /// Read the current tree value.
    Read,
    /// Write the specified value.
    Write(ValueHash),
}

/// Tags associated with a tree.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct TreeTags {
    pub architecture: String,
    pub depth: usize,
    pub hasher: String,
}

impl TreeTags {
    pub const ARCHITECTURE: &'static str = "AR16MT";

    pub fn new(hasher: &dyn HashTree) -> Self {
        Self {
            architecture: Self::ARCHITECTURE.to_owned(),
            hasher: hasher.name().to_owned(),
            depth: TREE_DEPTH,
        }
    }

    pub fn assert_consistency(&self, hasher: &dyn HashTree) {
        assert_eq!(
            self.architecture,
            Self::ARCHITECTURE,
            "Unsupported tree architecture `{}`, expected `{}`",
            self.architecture,
            Self::ARCHITECTURE
        );
        assert_eq!(
            self.depth, TREE_DEPTH,
            "Unexpected tree depth: expected {TREE_DEPTH}, got {}",
            self.depth
        );
        assert_eq!(
            hasher.name(),
            self.hasher,
            "Mismatch between the provided tree hasher `{}` and the hasher `{}` used \
             in the database",
            hasher.name(),
            self.hasher
        );
    }
}

/// Version-independent information about the tree.
#[derive(Debug, Default, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Manifest {
    // Number of tree versions stored in the database.
    pub(crate) version_count: u64,
    pub(crate) tags: Option<TreeTags>,
}

impl Manifest {
    #[cfg(test)]
    pub(crate) fn new(version_count: u64, hasher: &dyn HashTree) -> Self {
        Self {
            version_count,
            tags: Some(TreeTags::new(hasher)),
        }
    }
}

pub(crate) type NibblesBytes = [u8; KEY_SIZE];

/// Unversioned key (a sequence of nibbles) in a radix-16 Merkle tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Nibbles {
    nibble_count: usize,
    bytes: NibblesBytes,
}

impl Nibbles {
    pub const EMPTY: Self = Self {
        nibble_count: 0,
        bytes: [0_u8; KEY_SIZE],
    };

    pub fn nibble(key: &Key, index: usize) -> u8 {
        const NIBBLES_IN_U64: usize = 16; // 8 bytes * 2 nibbles / byte

        debug_assert!(index < 2 * KEY_SIZE);
        // Since the `Key` layout is little-endian, we reverse indexing of `u64`:
        // nibbles 0..=15 are in the final `u64`, etc.
        let u64_idx = 3 - index / NIBBLES_IN_U64;
        // The shift in `u64` is reversed as well: the 0th nibble needs the largest shift
        // `60 = 15 * 4`, the 1st one - 56, etc.
        let shift_in_u64 = (NIBBLES_IN_U64 - 1 - index % NIBBLES_IN_U64) * 4;
        ((key.0[u64_idx] >> shift_in_u64) & 0x0f) as u8
    }

    pub fn new(key: &Key, nibble_count: usize) -> Self {
        debug_assert!(nibble_count <= 2 * KEY_SIZE);
        let mut bytes = [0_u8; KEY_SIZE];
        key.to_big_endian(&mut bytes);

        // Unused bytes (and, if appropriate, the unused nibble in the last used byte)
        // needs to be zeroized in order for `Ord` / `Eq` / `Hash` traits to work properly.
        if nibble_count % 2 == 1 {
            bytes[nibble_count / 2] &= 0xf0;
        }
        let meaningful_bytes = (nibble_count + 1) / 2;
        for byte in bytes.iter_mut().skip(meaningful_bytes) {
            *byte = 0;
        }

        Self {
            nibble_count,
            bytes,
        }
    }

    pub fn from_parts(bytes: NibblesBytes, nibble_count: usize) -> Self {
        debug_assert!(nibble_count <= 2 * KEY_SIZE);
        Self {
            nibble_count,
            bytes,
        }
    }

    pub fn single(nibble: u8) -> Self {
        assert!(nibble < 16);
        let mut bytes = NibblesBytes::default();
        bytes[0] = nibble << 4;
        Self::from_parts(bytes, 1)
    }

    pub fn with_version(self, version: u64) -> NodeKey {
        NodeKey {
            version,
            nibbles: self,
        }
    }

    pub fn nibble_count(&self) -> usize {
        self.nibble_count
    }

    pub fn bytes(&self) -> &NibblesBytes {
        &self.bytes
    }

    /// Extracts the last nibble and the parent sequence of nibbles
    /// (i.e., one with the last nibble truncated). If this sequence of nibbles is empty,
    /// returns `None`.
    pub fn split_last(self) -> Option<(Self, u8)> {
        if self.nibble_count == 0 {
            return None;
        }

        let mut truncated_bytes = self.bytes;
        let last_byte_idx = (self.nibble_count - 1) / 2;
        let last_byte = self.bytes[last_byte_idx];
        let last_nibble = if self.nibble_count % 2 == 1 {
            truncated_bytes[last_byte_idx] = 0;
            last_byte >> 4
        } else {
            truncated_bytes[last_byte_idx] &= 0xf0;
            last_byte & 15
        };

        let parent = Self {
            nibble_count: self.nibble_count - 1,
            bytes: truncated_bytes,
        };
        Some((parent, last_nibble))
    }

    /// Pushes a nibble to the end of this sequence and returns the resulting nibble. Returns `None`
    /// if this nibble is full.
    pub fn push(self, nibble: u8) -> Option<Self> {
        if self.nibble_count == KEY_SIZE * 2 {
            return None;
        }

        let mut child = self;
        child.nibble_count += 1;
        let last_byte_idx = self.nibble_count / 2;
        if child.nibble_count % 2 == 0 {
            // The new `nibble` is 4 lower bits
            child.bytes[last_byte_idx] += nibble;
        } else {
            // The new `nibble` is 4 upper bits of a new byte
            child.bytes[last_byte_idx] = nibble << 4;
        }
        Some(child)
    }
}

impl fmt::Display for Nibbles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full_bytes = self.bytes.iter().take(self.nibble_count / 2);
        for &byte in full_bytes {
            write!(formatter, "{byte:02x}")?;
        }
        if self.nibble_count % 2 == 1 {
            let last_byte = self.bytes[self.nibble_count / 2];
            let last_nibble = last_byte >> 4;
            write!(formatter, "{last_nibble:x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Nibbles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Versioned key in a radix-16 Merkle tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey {
    pub(crate) version: u64,
    pub(crate) nibbles: Nibbles,
}

impl fmt::Debug for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.version, self.nibbles)
    }
}

impl NodeKey {
    pub(crate) const fn empty(version: u64) -> Self {
        Self {
            version,
            nibbles: Nibbles::EMPTY,
        }
    }

    pub(crate) fn from_db_key(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= 9, "`NodeKey` is too short");

        let version_bytes: [u8; 8] = bytes[..8].try_into().unwrap();
        let version = u64::from_be_bytes(version_bytes);
        let nibble_count = usize::from(bytes[8]);
        assert!(nibble_count <= 2 * KEY_SIZE);
        let nibbles_byte_len = (nibble_count + 1) / 2;
        assert_eq!(nibbles_byte_len, bytes.len() - 9);
        let mut nibbles = NibblesBytes::default();
        nibbles[..nibbles_byte_len].copy_from_slice(&bytes[9..]);

        Nibbles::from_parts(nibbles, nibble_count).with_version(version)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.nibbles.nibble_count == 0
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn to_db_key(self) -> Vec<u8> {
        let nibbles_byte_len = (self.nibbles.nibble_count + 1) / 2;
        // ^ equivalent to ceil(self.nibble_count / 2)
        let mut bytes = Vec::with_capacity(9 + nibbles_byte_len);
        // ^ 8 bytes for `version` + 1 byte for nibble count
        bytes.extend_from_slice(&self.version.to_be_bytes());
        bytes.push(self.nibbles.nibble_count as u8);
        // ^ conversion is safe: nibble_count <= 64
        bytes.extend_from_slice(&self.nibbles.bytes[..nibbles_byte_len]);
        bytes
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.version, self.nibbles)
    }
}

/// Key stored in the tree.
pub type Key = U256;
/// Hashed value stored in the tree.
pub type ValueHash = H256;

/// Leaf node of the tree.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct LeafNode {
    pub(crate) full_key: Key,
    pub(crate) value_hash: ValueHash,
    pub(crate) leaf_index: u64,
}

impl LeafNode {
    pub(crate) fn new(full_key: Key, value_hash: ValueHash, leaf_index: u64) -> Self {
        Self {
            full_key,
            value_hash,
            leaf_index,
        }
    }
}

/// Data of a leaf node of the tree.
#[derive(Debug, Clone, Copy)]
pub struct LeafData {
    pub value_hash: ValueHash,
    pub leaf_index: u64,
}

impl From<LeafNode> for LeafData {
    fn from(leaf: LeafNode) -> Self {
        Self {
            value_hash: leaf.value_hash,
            leaf_index: leaf.leaf_index,
        }
    }
}

/// Reference to a child in an [`InternalNode`].
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ChildRef {
    pub hash: ValueHash,
    pub version: u64,
    pub is_leaf: bool,
}

impl ChildRef {
    /// Creates a reference to a child with `value_hash` left blank (it will be computed later).
    pub fn leaf(version: u64) -> Self {
        Self {
            hash: ValueHash::default(),
            version,
            is_leaf: true,
        }
    }

    pub fn internal(version: u64) -> Self {
        Self {
            hash: ValueHash::default(),
            version,
            is_leaf: false,
        }
    }
}

/// Internal node in AR16MT containing up to 16 children.
#[derive(Default)]
pub struct InternalNode {
    children: SmallMap<ChildRef>,
    cache: Option<Box<InternalNodeCache>>,
}

impl Clone for InternalNode {
    fn clone(&self) -> Self {
        Self {
            children: self.children.clone(),
            cache: None,
            // ^ The cache shouldn't theoretically get invalidated as long as the tree operation
            // mode doesn't change, but we drop it just to be safe.
        }
    }
}

#[cfg(test)]
impl PartialEq for InternalNode {
    fn eq(&self, other: &Self) -> bool {
        self.children == other.children
    }
}

impl fmt::Debug for InternalNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = formatter.debug_map();
        for (nibble, child_ref) in self.children() {
            let nibble = format!("{nibble:x}");
            map.entry(&nibble, child_ref);
        }
        map.finish()
    }
}

impl InternalNode {
    /// Number of children in an internal node (= tree radix).
    pub(crate) const CHILD_COUNT: u8 = 16;

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            cache: None,
            children: SmallMap::with_capacity(capacity),
        }
    }

    pub(crate) fn child_count(&self) -> usize {
        self.children.len()
    }

    pub(crate) fn cache_mut(&mut self) -> Option<&mut InternalNodeCache> {
        self.cache.as_deref_mut()
    }

    pub(crate) fn set_cache(&mut self, cache: Box<InternalNodeCache>) -> &mut InternalNodeCache {
        debug_assert!(self.cache.is_none());
        self.cache.get_or_insert(cache)
    }

    pub(crate) fn children(&self) -> impl Iterator<Item = (u8, &ChildRef)> + '_ {
        self.children.iter()
    }

    pub(crate) fn child_refs(&self) -> impl Iterator<Item = &ChildRef> + '_ {
        self.children.values()
    }

    pub(crate) fn child_hashes(&self) -> [Option<ValueHash>; Self::CHILD_COUNT as usize] {
        let mut hashes = [None; Self::CHILD_COUNT as usize];
        for (nibble, child_ref) in self.children.iter() {
            hashes[nibble as usize] = Some(child_ref.hash);
        }
        hashes
    }

    pub(crate) fn child_ref(&self, nibble: u8) -> Option<&ChildRef> {
        self.children.get(nibble)
    }

    pub(crate) fn child_ref_mut(&mut self, nibble: u8) -> Option<&mut ChildRef> {
        self.children.get_mut(nibble)
    }

    pub(crate) fn insert_child_ref(&mut self, nibble: u8, child_ref: ChildRef) {
        self.children.insert(nibble, child_ref);
    }
}

/// Tree node (either a leaf or an internal node).
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Node {
    /// Internal node.
    Internal(InternalNode),
    /// Tree leaf.
    Leaf(LeafNode),
}

impl From<LeafNode> for Node {
    fn from(leaf: LeafNode) -> Self {
        Self::Leaf(leaf)
    }
}

impl From<InternalNode> for Node {
    fn from(node: InternalNode) -> Self {
        Self::Internal(node)
    }
}

/// Root node of the tree. Besides a [`Node`], contains the general information about the tree
/// (e.g., the number of leaves).
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Root {
    /// Root for an empty tree.
    Empty,
    /// Root for a tree with at least one leaf.
    Filled {
        /// Number of leaves in the tree.
        leaf_count: NonZeroU64,
        /// Root node of the tree.
        node: Node,
    },
}

impl Root {
    pub(crate) fn new(leaf_count: u64, node: Node) -> Self {
        Self::Filled {
            leaf_count: NonZeroU64::new(leaf_count).unwrap(),
            node,
        }
    }

    pub(crate) fn leaf_count(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::Filled { leaf_count, .. } => (*leaf_count).into(),
        }
    }
}

/// Stale [`NodeKey`] with information about when it was replaced.
#[derive(Debug)]
pub(crate) struct StaleNodeKey {
    /// Replaced key.
    pub key: NodeKey,
    /// Version of the tree at which it was replaced.
    pub replaced_in_version: u64,
}

impl StaleNodeKey {
    pub fn new(key: NodeKey, replaced_in_version: u64) -> Self {
        Self {
            key,
            replaced_in_version,
        }
    }

    pub fn to_db_key(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.replaced_in_version.to_be_bytes());
        bytes.extend_from_slice(&self.key.to_db_key());
        bytes
    }
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
    Inserted {
        /// Index of the inserted node.
        leaf_index: u64,
    },
    /// A node with the specified index was updated.
    Updated {
        /// Index of the updated node.
        leaf_index: u64,
        /// Hash of the previous value.
        previous_value: ValueHash,
    },
    /// A node was read from the tree.
    Read {
        /// Index of the read node.
        leaf_index: u64,
        /// Hash of the read value.
        value: ValueHash,
    },
    /// A missing key was read.
    ReadMissingKey,
}

impl TreeLogEntry {
    pub(crate) fn insert(leaf_index: u64) -> Self {
        Self::Inserted { leaf_index }
    }

    pub(crate) fn update(previous_value: ValueHash, leaf_index: u64) -> Self {
        Self::Updated {
            leaf_index,
            previous_value,
        }
    }

    pub(crate) fn read(value: ValueHash, leaf_index: u64) -> Self {
        Self::Read { leaf_index, value }
    }

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
    /// Merkle path to prove the log authenticity. The path consists of up to 256 hashes
    /// ordered starting the bottommost level of the tree (one with leaves) and ending before
    /// the root level.
    ///
    /// If the path is not full (contains <256 hashes), it means that the hashes at the beginning
    /// corresponding to the empty subtrees are skipped. This allows compacting the proof ~10x.
    pub merkle_path: P,
    /// Root tree hash after the operation.
    pub root_hash: ValueHash,
}

#[cfg(test)]
mod tests {
    use super::*;

    // `U256` uses little-endian `u64` ordering; i.e., this is
    // 0x_dead_beef_0000_0000_.._0000.
    const TEST_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0000_0000]);

    #[test]
    fn accessing_nibbles_in_key() {
        let start_nibbles = [0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf];
        for (i, nibble) in start_nibbles.into_iter().enumerate() {
            assert_eq!(Nibbles::nibble(&TEST_KEY, i), nibble);
        }
        for i in 8..(2 * KEY_SIZE) {
            assert_eq!(Nibbles::nibble(&TEST_KEY, i), 0);
        }
    }

    #[test]
    fn nibbles_and_node_key_display() {
        let nibbles = Nibbles::new(&TEST_KEY, 5);
        assert_eq!(nibbles.to_string(), "deadb");

        let nibbles = Nibbles::new(&TEST_KEY, 6);
        assert_eq!(nibbles.to_string(), "deadbe");

        let nibbles = Nibbles::new(&TEST_KEY, 9);
        assert_eq!(nibbles.to_string(), "deadbeef0");

        let node_key = nibbles.with_version(3);
        assert_eq!(node_key.to_string(), "3:deadbeef0");
    }

    #[test]
    fn manipulating_nibbles() {
        let nibbles = Nibbles::new(&TEST_KEY, 0);
        let child = nibbles.push(0xb).unwrap();
        assert_eq!(child.to_string(), "b");

        let nibbles = Nibbles::new(&TEST_KEY, 6);
        let child = nibbles.push(0xb).unwrap();
        assert_eq!(child.to_string(), "deadbeb");

        let nibbles = Nibbles::new(&TEST_KEY, 7);
        let child = nibbles.push(0xb).unwrap();
        assert_eq!(child.to_string(), "deadbeeb");

        let nibbles = Nibbles::new(&TEST_KEY, 64);
        assert!(nibbles.push(0xb).is_none());
    }

    #[test]
    fn node_key_serialization() {
        let nibbles = Nibbles::new(&TEST_KEY, 6);
        let node_key = nibbles.with_version(3);

        let serialized_key = node_key.to_db_key();
        assert_eq!(
            serialized_key,
            [0, 0, 0, 0, 0, 0, 0, 3, 6, 0xde, 0xad, 0xbe]
        );
        // ^ big-endian u64 version, then u8 nibble count, then nibbles
        let key_copy = NodeKey::from_db_key(&serialized_key);
        assert_eq!(key_copy, node_key);

        let nibbles = Nibbles::new(&TEST_KEY, 7);
        let node_key = nibbles.with_version(3);

        let serialized_key = node_key.to_db_key();
        assert_eq!(
            serialized_key,
            [0, 0, 0, 0, 0, 0, 0, 3, 7, 0xde, 0xad, 0xbe, 0xe0]
        );
        // ^ the last byte must be truncated
        let key_copy = NodeKey::from_db_key(&serialized_key);
        assert_eq!(key_copy, node_key);

        let empty_key = Nibbles::EMPTY.with_version(42);
        let serialized_key = empty_key.to_db_key();
        assert_eq!(serialized_key, [0, 0, 0, 0, 0, 0, 0, 42, 0]);
        let key_copy = NodeKey::from_db_key(&serialized_key);
        assert_eq!(key_copy, empty_key);
    }

    #[test]
    fn nibbles_created_from_different_sources_can_be_equal() {
        let nibbles = Nibbles::new(&TEST_KEY, 1);
        let other_key = U256([0, 0, 0, 0x_d000_0000_0000_0000]);
        let other_nibbles = Nibbles::new(&other_key, 1);
        assert_eq!(nibbles, other_nibbles);

        let nibbles = Nibbles::new(&TEST_KEY, 2);
        let other_nibbles = Nibbles::new(&other_key, 2);
        assert_ne!(nibbles, other_nibbles);
        assert!(nibbles > other_nibbles);
    }
}

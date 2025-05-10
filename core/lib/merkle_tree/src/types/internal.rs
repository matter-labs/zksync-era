//! Internal types, mostly related to Merkle tree nodes. Note that because of the public `Database` trait,
//! some of these types are declared as public and can be even exported using the `unstable` module.
//! Still, logically these types are private, so adding them to new public APIs etc. is a logical error.

use std::{collections::HashMap, fmt, num::NonZeroU64, str::FromStr};

use anyhow::Context;

use crate::{
    hasher::{HashTree, InternalNodeCache},
    types::{Key, TreeEntry, ValueHash},
    utils::SmallMap,
};

/// Size of a (leaf) tree key in bytes.
pub(crate) const KEY_SIZE: usize = 32;
/// Depth of the tree (= number of bits in `KEY_SIZE`).
pub(crate) const TREE_DEPTH: usize = KEY_SIZE * 8;
/// Size of a hashed value in bytes.
pub(crate) const HASH_SIZE: usize = 32;

/// Tags associated with a tree.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct TreeTags {
    pub architecture: String,
    pub depth: usize,
    pub hasher: String,
    pub is_recovering: bool,
    /// Custom / user-defined tags.
    pub custom: HashMap<String, String>,
}

impl TreeTags {
    pub const ARCHITECTURE: &'static str = "AR16MT";

    pub fn new(hasher: &dyn HashTree) -> Self {
        Self {
            architecture: Self::ARCHITECTURE.to_owned(),
            hasher: hasher.name().to_owned(),
            depth: TREE_DEPTH,
            is_recovering: false,
            custom: HashMap::new(),
        }
    }

    pub fn ensure_consistency(
        &self,
        hasher: &dyn HashTree,
        expecting_recovery: bool,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.architecture == Self::ARCHITECTURE,
            "Unsupported tree architecture `{}`, expected `{}`",
            self.architecture,
            Self::ARCHITECTURE
        );
        anyhow::ensure!(
            self.depth == TREE_DEPTH,
            "Unexpected tree depth: expected {TREE_DEPTH}, got {}",
            self.depth
        );
        anyhow::ensure!(
            hasher.name() == self.hasher,
            "Mismatch between the provided tree hasher `{}` and the hasher `{}` used \
             in the database",
            hasher.name(),
            self.hasher
        );

        if expecting_recovery {
            anyhow::ensure!(
                self.is_recovering,
                "Tree is expected to be in the process of recovery, but it is not"
            );
        } else {
            anyhow::ensure!(
                !self.is_recovering,
                "Tree is being recovered; cannot access it until recovery finishes"
            );
        }
        Ok(())
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
    /// Returns the version of the tree that is currently being recovered.
    pub fn recovered_version(&self) -> Option<u64> {
        if self.tags.as_ref()?.is_recovering {
            Some(self.version_count.checked_sub(1)?)
        } else {
            None
        }
    }

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
        let meaningful_bytes = nibble_count.div_ceil(2);
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

    /// Returns nibbles that form a common prefix between these nibbles and the provided `key`.
    pub fn common_prefix(mut self, other: &Self) -> Self {
        for i in 0..self.nibble_count.div_ceil(2) {
            let (this_byte, other_byte) = (self.bytes[i], other.bytes[i]);
            if this_byte != other_byte {
                // Check whether the first nibble matches.
                if this_byte & 0xf0 == other_byte & 0xf0 {
                    self.nibble_count = i * 2 + 1;
                    self.bytes[i] &= 0xf0;
                    self.bytes[(i + 1)..].fill(0);
                } else {
                    self.nibble_count = i * 2;
                    self.bytes[i..].fill(0);
                }
                return self;
            }
        }
        self
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

impl FromStr for Nibbles {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        anyhow::ensure!(s.len() <= KEY_SIZE * 2, "too many nibbles");
        let mut bytes = NibblesBytes::default();
        for (i, byte) in s.bytes().enumerate() {
            let nibble = match byte {
                b'0'..=b'9' => byte - b'0',
                b'A'..=b'F' => byte - b'A' + 10,
                b'a'..=b'f' => byte - b'a' + 10,
                _ => anyhow::bail!("unexpected nibble: {byte:?}"),
            };

            assert!(nibble < 16);
            if i % 2 == 0 {
                bytes[i / 2] = nibble * 16;
            } else {
                bytes[i / 2] += nibble;
            }
        }
        Ok(Self {
            nibble_count: s.len(),
            bytes,
        })
    }
}

/// Versioned key in a radix-16 Merkle tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey {
    pub(crate) version: u64,
    pub(crate) nibbles: Nibbles,
}

impl fmt::Display for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.version, self.nibbles)
    }
}

impl fmt::Debug for NodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl FromStr for NodeKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (version, nibbles) = s
            .split_once(':')
            .context("node key does not contain `:` delimiter")?;
        let version = version.parse().context("invalid key version")?;
        let nibbles = nibbles.parse().context("invalid nibbles")?;
        Ok(Self { version, nibbles })
    }
}

impl NodeKey {
    pub(crate) const fn empty(version: u64) -> Self {
        Self {
            version,
            nibbles: Nibbles::EMPTY,
        }
    }

    // TODO (BFT-239): Add a fallible version for verifying consistency
    pub(crate) fn from_db_key(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= 9, "`NodeKey` is too short");

        let version_bytes: [u8; 8] = bytes[..8].try_into().unwrap();
        let version = u64::from_be_bytes(version_bytes);
        let nibble_count = usize::from(bytes[8]);
        assert!(nibble_count <= 2 * KEY_SIZE);
        let nibbles_byte_len = nibble_count.div_ceil(2);
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
        let nibbles_byte_len = self.nibbles.nibble_count.div_ceil(2);
        // ^ equivalent to `ceil(self.nibble_count / 2)`
        let mut bytes = Vec::with_capacity(9 + nibbles_byte_len);
        // ^ 8 bytes for `version` + 1 byte for nibble count
        bytes.extend_from_slice(&self.version.to_be_bytes());
        bytes.push(self.nibbles.nibble_count as u8);
        // ^ conversion is safe: `nibble_count <= 64`
        bytes.extend_from_slice(&self.nibbles.bytes[..nibbles_byte_len]);
        bytes
    }
}

/// Leaf node of the tree.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct LeafNode {
    pub full_key: Key,
    pub value_hash: ValueHash,
    pub leaf_index: u64,
}

impl LeafNode {
    pub(crate) fn new(entry: TreeEntry) -> Self {
        Self {
            full_key: entry.key,
            value_hash: entry.value,
            leaf_index: entry.leaf_index,
        }
    }

    pub(crate) fn update_from(&mut self, entry: TreeEntry) {
        self.value_hash = entry.value;
        self.leaf_index = entry.leaf_index;
    }
}

/// Reference to a child in an [`InternalNode`].
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ChildRef {
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

    pub fn children(&self) -> impl Iterator<Item = (u8, &ChildRef)> + '_ {
        self.children.iter()
    }

    pub(crate) fn child_refs(&self) -> impl Iterator<Item = &ChildRef> + '_ {
        self.children.values()
    }

    #[cfg(test)]
    pub(crate) fn child_refs_mut(&mut self) -> impl Iterator<Item = &mut ChildRef> + '_ {
        self.children.values_mut()
    }

    pub(crate) fn last_child_ref(&self) -> (u8, &ChildRef) {
        self.children.last().unwrap()
        // ^ `unwrap()` is safe by construction; all persisted internal nodes are not empty
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

/// Raw node fetched from a database.
#[derive(Debug)]
pub struct RawNode {
    /// Bytes for a serialized node.
    pub raw: Vec<u8>,
    /// Leaf if a node can be deserialized into it.
    pub leaf: Option<LeafNode>,
    /// Internal node if a node can be deserialized into it.
    pub internal: Option<InternalNode>,
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

/// Profiled Merkle tree operation used in `Database::start_profiling()`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ProfiledTreeOperation {
    /// Loading ancestors for nodes inserted or updated in the tree.
    LoadAncestors,
    /// Getting entries from the tree without Merkle proofs.
    GetEntries,
    /// Getting entries from the tree with Merkle proofs.
    GetEntriesWithProofs,
}

impl ProfiledTreeOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::LoadAncestors => "load_ancestors",
            Self::GetEntries => "get_entries",
            Self::GetEntriesWithProofs => "get_entries_with_proofs",
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::U256;

    use super::*;

    // `U256` uses little-endian `u64` ordering; i.e., this is
    // `0x_dead_beef_0000_0000_.._0000.`
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
        let restored: Nibbles = nibbles.to_string().parse().unwrap();
        assert_eq!(restored, nibbles);

        let nibbles = Nibbles::new(&TEST_KEY, 6);
        assert_eq!(nibbles.to_string(), "deadbe");
        let restored: Nibbles = nibbles.to_string().parse().unwrap();
        assert_eq!(restored, nibbles);

        let nibbles = Nibbles::new(&TEST_KEY, 9);
        assert_eq!(nibbles.to_string(), "deadbeef0");
        let restored: Nibbles = nibbles.to_string().parse().unwrap();
        assert_eq!(restored, nibbles);

        let node_key = nibbles.with_version(3);
        assert_eq!(node_key.to_string(), "3:deadbeef0");
        let restored: NodeKey = node_key.to_string().parse().unwrap();
        assert_eq!(restored, node_key);
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
    fn nibbles_prefix() {
        let nibbles = Nibbles::new(&TEST_KEY, 6);
        assert_eq!(nibbles.common_prefix(&nibbles), nibbles);

        let nibbles = Nibbles::new(&TEST_KEY, 6);
        let prefix = Nibbles::new(&TEST_KEY, 4);
        assert_eq!(nibbles.common_prefix(&prefix), prefix);
        assert_eq!(prefix.common_prefix(&nibbles), prefix);

        let nibbles = Nibbles::new(&TEST_KEY, 7);
        assert_eq!(nibbles.common_prefix(&prefix), prefix);
        assert_eq!(prefix.common_prefix(&nibbles), prefix);

        let nibbles = Nibbles::new(&TEST_KEY, 64);
        let diverging_nibbles = Nibbles::new(&TEST_KEY, 4).push(0x1).unwrap();
        assert_eq!(nibbles.common_prefix(&diverging_nibbles), prefix);

        let diverging_nibbles = Nibbles::new(&TEST_KEY, 5).push(0x1).unwrap();
        assert_eq!(
            nibbles.common_prefix(&diverging_nibbles),
            Nibbles::new(&TEST_KEY, 5)
        );

        let diverging_nibbles = Nibbles::from_parts([0xff; KEY_SIZE], 64);
        assert_eq!(nibbles.common_prefix(&diverging_nibbles), Nibbles::EMPTY);
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

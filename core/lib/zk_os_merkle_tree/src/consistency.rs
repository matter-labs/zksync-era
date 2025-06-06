use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use zksync_basic_types::H256;

use crate::{
    leaf_nibbles, max_nibbles_for_internal_node, max_node_children,
    types::{InternalNode, KeyLookup, Leaf, Node, NodeKey},
    Database, DeserializeError, HashTree, MerkleTree, TreeParams,
};

#[derive(Debug, Clone, Copy)]
pub enum IndexKind {
    This,
    Next,
}

impl fmt::Display for IndexKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::This => "self",
            Self::Next => "next",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConsistencyError {
    #[error("failed deserializing node from DB: {0}")]
    Deserialize(#[from] DeserializeError),
    #[error("tree version {0} does not exist")]
    MissingVersion(u64),
    #[error("missing root for tree version {0}")]
    MissingRoot(u64),
    #[error("missing min / max guards")]
    NoGuards,

    #[error("internal node with key {key} has unexpected number of children: expected {expected}, actual {actual}")]
    ChildCountMismatch {
        key: NodeKey,
        expected: usize,
        actual: usize,
    },
    #[error(
        "internal node with key {key} should have version {expected_version} (max among child ref versions)"
    )]
    KeyVersionMismatch { key: NodeKey, expected_version: u64 },
    #[error("root node should have version >={max_child_version} (max among child ref versions)")]
    RootVersionMismatch { max_child_version: u64 },

    #[error("unexpected min guard (leaf with index 0)")]
    UnexpectedMinGuard,
    #[error("leaf {0} has minimum key")]
    MinKey(u64),
    #[error("unexpected max guard (leaf with index 1)")]
    UnexpectedMaxGuard,
    #[error("leaf {0} has maximum key")]
    MaxKey(u64),
    #[error("missing key {0:?} in lookup")]
    MissingKeyLookup(H256),
    #[error(
        "{kind} index mismatch for key {key:?} in tree leaf ({in_tree}) and lookup ({in_lookup})"
    )]
    IndexMismatch {
        kind: IndexKind,
        key: H256,
        in_lookup: u64,
        in_tree: u64,
    },
    #[error("{index_kind} index for leaf #{leaf_index} ({index}) >= leaf count ({leaf_count})")]
    LeafIndexOverflow {
        leaf_index: u64,
        index_kind: IndexKind,
        index: u64,
        leaf_count: u64,
    },
    #[error("{index_kind} index for leaf #{leaf_index} ({index}) is a duplicate")]
    DuplicateLeafIndex {
        leaf_index: u64,
        index_kind: IndexKind,
        index: u64,
    },
    #[error("{index_kind} index for leaf #{leaf_index} points to the disallowed guard")]
    IncorrectGuardRef {
        leaf_index: u64,
        index_kind: IndexKind,
    },

    #[error(
        "internal node at {key} specifies that child hash at `{nibble}` \
         is {expected}, but it actually is {actual}"
    )]
    HashMismatch {
        key: NodeKey,
        nibble: u8,
        expected: H256,
        actual: H256,
    },
}

impl<DB: Database, P: TreeParams> MerkleTree<DB, P> {
    /// Verifies the internal tree consistency as stored in the database.
    ///
    /// If `validate_indices` flag is set, it will be checked that indices for all tree leaves are unique
    /// and are sequentially assigned starting from 1.
    ///
    /// # Errors
    ///
    /// Returns an error (the first encountered one if there are multiple).
    pub fn verify_consistency(&self, version: u64) -> Result<(), ConsistencyError> {
        let manifest = self.db.try_manifest()?;
        let manifest = manifest.ok_or(ConsistencyError::MissingVersion(version))?;
        if version >= manifest.version_count {
            return Err(ConsistencyError::MissingVersion(version));
        }

        let root = self
            .db
            .try_root(version)?
            .ok_or(ConsistencyError::MissingRoot(version))?;

        if root.leaf_count < 2 {
            return Err(ConsistencyError::NoGuards);
        }
        let leaf_data = LeafConsistencyData::new(root.leaf_count);

        // We want to perform a depth-first walk of the tree in order to not keep
        // much in memory.
        let root_key = NodeKey::root(version);
        self.validate_internal_node(&root.root_node, root_key, &leaf_data)?;

        Ok(())
    }

    fn validate_internal_node(
        &self,
        node: &InternalNode,
        key: NodeKey,
        leaf_data: &LeafConsistencyData,
    ) -> Result<H256, ConsistencyError> {
        use rayon::prelude::*;

        let leaf_count = leaf_data.expected_leaf_count;
        assert!(leaf_count > 0); // checked during initialization
        let child_depth =
            (max_nibbles_for_internal_node::<P>() - key.nibble_count) * P::INTERNAL_NODE_DEPTH;
        let last_child_index = (leaf_count - 1) >> child_depth;
        let last_index_on_level = last_child_index / u64::from(max_node_children::<P>());

        assert!(key.index_on_level <= last_index_on_level);
        let expected_child_count = if key.index_on_level < last_index_on_level {
            max_node_children::<P>().into()
        } else {
            (last_child_index % u64::from(max_node_children::<P>())) as usize + 1
        };

        if node.children.len() != expected_child_count {
            return Err(ConsistencyError::ChildCountMismatch {
                key,
                expected: expected_child_count,
                actual: node.children.len(),
            });
        }

        let expected_version = node
            .children
            .iter()
            .map(|child_ref| child_ref.version)
            .max()
            .unwrap();
        if key.nibble_count != 0 && expected_version != key.version {
            return Err(ConsistencyError::KeyVersionMismatch {
                key,
                expected_version,
            });
        } else if key.nibble_count == 0 && expected_version > key.version {
            return Err(ConsistencyError::RootVersionMismatch {
                max_child_version: expected_version,
            });
        }

        // `.into_par_iter()` below is the only place where `rayon`-based parallelism
        // is used in tree verification.
        node.children
            .par_iter()
            .enumerate()
            .try_for_each(|(i, child_ref)| {
                let child_key = NodeKey {
                    version: child_ref.version,
                    nibble_count: key.nibble_count + 1,
                    index_on_level: i as u64 + (key.index_on_level << P::INTERNAL_NODE_DEPTH),
                };
                let children = self.db.try_nodes(&[child_key])?;

                // Assertions are used below because they are a part of the `Database` contract.
                assert_eq!(children.len(), 1);
                let child = children.into_iter().next().unwrap();

                // Recursion here is OK; the tree isn't that deep.
                let child_hash = match &child {
                    Node::Internal(node) => {
                        assert!(child_key.nibble_count <= max_nibbles_for_internal_node::<P>());
                        self.validate_internal_node(node, child_key, leaf_data)?
                    }
                    Node::Leaf(leaf) => {
                        assert_eq!(child_key.nibble_count, leaf_nibbles::<P>());
                        self.validate_leaf(leaf, child_key, leaf_data)?
                    }
                };

                if child_hash == child_ref.hash {
                    Ok(())
                } else {
                    Err(ConsistencyError::HashMismatch {
                        key,
                        nibble: i as u8,
                        expected: child_ref.hash,
                        actual: child_hash,
                    })
                }
            })?;

        Ok(node.hash::<P>(&self.hasher, child_depth))
    }

    fn validate_leaf(
        &self,
        leaf: &Leaf,
        key: NodeKey,
        leaf_data: &LeafConsistencyData,
    ) -> Result<H256, ConsistencyError> {
        let index = key.index_on_level;

        if index == 0 && leaf.key != H256::zero() {
            return Err(ConsistencyError::UnexpectedMinGuard);
        }
        if index == 1 && (leaf.key != H256::repeat_byte(0xff) || leaf.next_index != 1) {
            return Err(ConsistencyError::UnexpectedMaxGuard);
        }

        leaf_data.insert_leaf(leaf, key.index_on_level)?;

        let lookup = self.db.indices(key.version, &[leaf.key])?;
        assert_eq!(lookup.len(), 1);
        let lookup_index = match lookup.into_iter().next().unwrap() {
            KeyLookup::Existing(idx) => idx,
            KeyLookup::Missing { .. } => {
                return Err(ConsistencyError::MissingKeyLookup(leaf.key));
            }
        };
        if lookup_index != index {
            return Err(ConsistencyError::IndexMismatch {
                kind: IndexKind::This,
                key: leaf.key,
                in_lookup: lookup_index,
                in_tree: index,
            });
        }

        if index != 1 {
            let next_key = next_key(leaf.key).ok_or(ConsistencyError::MaxKey(index))?;
            let lookup = self.db.indices(key.version, &[next_key])?;
            assert_eq!(lookup.len(), 1);

            let lookup_next_index = match lookup.into_iter().next().unwrap() {
                KeyLookup::Existing(idx) => idx,
                KeyLookup::Missing {
                    next_key_and_index: (next_key, idx),
                    ..
                } => {
                    assert!(next_key > leaf.key);
                    idx
                }
            };

            if lookup_next_index != leaf.next_index {
                return Err(ConsistencyError::IndexMismatch {
                    kind: IndexKind::Next,
                    key: leaf.key,
                    in_lookup: lookup_next_index,
                    in_tree: leaf.next_index,
                });
            }
        }

        Ok(self.hasher.hash_leaf(leaf))
    }
}

fn next_key(key: H256) -> Option<H256> {
    let mut bytes = key.0;
    for pos in (0..32).rev() {
        if bytes[pos] != u8::MAX {
            bytes[pos] += 1;
            for byte in &mut bytes[pos + 1..] {
                *byte = 0;
            }
            return Some(H256(bytes));
        }
    }
    None
}

#[must_use = "Final checks should be performed in `finalize()`"]
#[derive(Debug)]
struct LeafConsistencyData {
    expected_leaf_count: u64,
    next_indices_set: AtomicBitSet,
}

impl LeafConsistencyData {
    fn new(expected_leaf_count: u64) -> Self {
        Self {
            expected_leaf_count,
            next_indices_set: AtomicBitSet::new(expected_leaf_count as usize),
        }
    }

    fn insert_leaf(&self, leaf: &Leaf, leaf_index: u64) -> Result<(), ConsistencyError> {
        if leaf_index != 1 {
            self.insert_into_set(
                &self.next_indices_set,
                IndexKind::Next,
                leaf_index,
                leaf.next_index,
            )?;
        }
        Ok(())
    }

    fn insert_into_set(
        &self,
        bit_set: &AtomicBitSet,
        index_kind: IndexKind,
        leaf_index: u64,
        index: u64,
    ) -> Result<(), ConsistencyError> {
        if index >= self.expected_leaf_count {
            return Err(ConsistencyError::LeafIndexOverflow {
                leaf_index,
                index_kind,
                index,
                leaf_count: self.expected_leaf_count,
            });
        }

        match index_kind {
            IndexKind::Next if index == 0 => {
                return Err(ConsistencyError::IncorrectGuardRef {
                    leaf_index,
                    index_kind,
                });
            }
            _ => { /* do nothing */ }
        }

        if bit_set.set(index as usize) {
            return Err(ConsistencyError::DuplicateLeafIndex {
                leaf_index,
                index_kind,
                index,
            });
        }
        Ok(())
    }
}

/// Primitive atomic bit set implementation that only supports setting bits.
#[derive(Debug)]
struct AtomicBitSet {
    bits: Vec<AtomicU64>,
}

impl AtomicBitSet {
    const BITS_PER_ATOMIC: usize = 64;

    fn new(len: usize) -> Self {
        let atomic_count = len.div_ceil(Self::BITS_PER_ATOMIC);
        let mut bits = Vec::with_capacity(atomic_count);
        bits.resize_with(atomic_count, AtomicU64::default);
        Self { bits }
    }

    /// Returns the previous bit value.
    fn set(&self, bit_index: usize) -> bool {
        let atomic_index = bit_index / Self::BITS_PER_ATOMIC;
        let shift_in_atomic = bit_index % Self::BITS_PER_ATOMIC;
        let atomic = &self.bits[atomic_index];
        let mask = 1 << (shift_in_atomic as u64);
        let prev_value = atomic.fetch_or(mask, Ordering::SeqCst);
        prev_value & mask != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_and_next_key_work_as_expected() {
        let key = H256::zero();
        assert_eq!(next_key(key), Some(H256::from_low_u64_be(1)));

        let key = H256::from_low_u64_be(10);
        assert_eq!(next_key(key), Some(H256::from_low_u64_be(11)));

        let key = H256::from_low_u64_be((1 << 32) - 1);
        assert_eq!(next_key(key), Some(H256::from_low_u64_be(1 << 32)));

        let key = H256::from_low_u64_be(1 << 32);
        assert_eq!(next_key(key), Some(H256::from_low_u64_be((1 << 32) + 1)));

        let key = H256::repeat_byte(0xff);
        assert_eq!(next_key(key), None);
    }
}

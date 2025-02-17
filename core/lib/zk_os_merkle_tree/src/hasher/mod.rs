use std::iter;

use once_cell::sync::Lazy;
use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};

use crate::types::{Leaf, MAX_TREE_DEPTH};

mod nodes;

/// Tree hashing functionality.
pub trait HashTree: Send + Sync {
    /// Returns the unique name of the hasher. This is used in Merkle tree tags to ensure
    /// that the tree remains consistent.
    fn name(&self) -> &'static str;

    /// Hashes a leaf node.
    fn hash_leaf(&self, leaf: &Leaf) -> H256;
    /// Compresses hashes in an intermediate node of a binary Merkle tree.
    fn hash_branch(&self, lhs: &H256, rhs: &H256) -> H256;

    /// Returns the hash of an empty subtree with the given depth. `depth == 0` corresponds to leaves. Implementations
    /// are encouraged to cache the returned values.
    ///
    /// Guaranteed to never be called with `depth > 64` (i.e., exceeding the depth of the entire tree).
    fn empty_subtree_hash(&self, depth: u8) -> H256;
}

impl<H: HashTree + ?Sized> HashTree for &H {
    fn name(&self) -> &'static str {
        (**self).name()
    }

    fn hash_leaf(&self, leaf: &Leaf) -> H256 {
        (**self).hash_leaf(leaf)
    }

    fn hash_branch(&self, lhs: &H256, rhs: &H256) -> H256 {
        (**self).hash_branch(lhs, rhs)
    }

    fn empty_subtree_hash(&self, depth: u8) -> H256 {
        (**self).empty_subtree_hash(depth)
    }
}

/// No-op implementation.
impl HashTree for () {
    fn name(&self) -> &'static str {
        "no-op"
    }

    fn hash_leaf(&self, _leaf: &Leaf) -> H256 {
        H256::zero()
    }

    fn hash_branch(&self, _lhs: &H256, _rhs: &H256) -> H256 {
        H256::zero()
    }

    fn empty_subtree_hash(&self, _depth: u8) -> H256 {
        H256::zero()
    }
}

impl HashTree for Blake2Hasher {
    fn name(&self) -> &'static str {
        "Blake2s256"
    }

    fn hash_leaf(&self, leaf: &Leaf) -> H256 {
        let mut hashed_bytes = [0; 2 * 32 + 2 * 8];
        hashed_bytes[..32].copy_from_slice(leaf.key.as_bytes());
        hashed_bytes[32..64].copy_from_slice(leaf.value.as_bytes());
        hashed_bytes[64..72].copy_from_slice(&leaf.prev_index.to_le_bytes());
        hashed_bytes[72..].copy_from_slice(&leaf.next_index.to_le_bytes());
        self.hash_bytes(&hashed_bytes)
    }

    fn hash_branch(&self, lhs: &H256, rhs: &H256) -> H256 {
        self.compress(lhs, rhs)
    }

    fn empty_subtree_hash(&self, depth: u8) -> H256 {
        static EMPTY_TREE_HASHES: Lazy<Vec<H256>> = Lazy::new(compute_empty_tree_hashes);
        EMPTY_TREE_HASHES[usize::from(depth)]
    }
}

fn compute_empty_tree_hashes() -> Vec<H256> {
    let empty_leaf_hash = Blake2Hasher.hash_bytes(&[0_u8; 2 * 32 + 2 * 8]);
    iter::successors(Some(empty_leaf_hash), |hash| {
        Some(Blake2Hasher.hash_branch(hash, hash))
    })
    .take(usize::from(MAX_TREE_DEPTH) + 1)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_leaves_is_correct() {
        let expected_empty_leaf_hash: H256 =
            "0xc4fde76a8d68422c5fbafde250f492109fb29ac66753292e1153aa11adae1a3a"
                .parse()
                .unwrap();
        assert_eq!(Blake2Hasher.empty_subtree_hash(0), expected_empty_leaf_hash);

        let expected_level1_empty_hash: H256 =
            "0xd53cc61c1aba0c548d73b0131e635e3110434a9c13c65cae08ed7da60ad2858f"
                .parse()
                .unwrap();
        assert_eq!(
            Blake2Hasher.empty_subtree_hash(1),
            expected_level1_empty_hash
        );

        let expected_level63_empty_hash: H256 =
            "0x59841e10b053bb976a3a159af345e27cc4dbbb1f5424051b6d24f5c56b69e74d"
                .parse()
                .unwrap();
        assert_eq!(
            Blake2Hasher.empty_subtree_hash(63),
            expected_level63_empty_hash
        );

        let expected_min_guard_hash: H256 =
            "0x4034715b557ca4bc5aef36ae5f28223ab27da4ac291cc63d0835ef2e0eba0c42"
                .parse()
                .unwrap();
        assert_eq!(
            Blake2Hasher.hash_leaf(&Leaf::MIN_GUARD),
            expected_min_guard_hash
        );

        let expected_max_guard_hash: H256 =
            "0xb30053e4154d49d35b0005e3ee0d4e0fc9fd330aed004c86810b57cf40a28afa"
                .parse()
                .unwrap();
        assert_eq!(
            Blake2Hasher.hash_leaf(&Leaf::MAX_GUARD),
            expected_max_guard_hash
        );
    }
}

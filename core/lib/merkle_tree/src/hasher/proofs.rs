//! Merkle proof-related hashing logic.

use std::mem;

use crate::{
    hasher::{HashTree, HasherWithStats},
    types::{
        BlockOutputWithProofs, Key, LeafNode, TreeEntry, TreeEntryWithProof, TreeInstruction,
        TreeLogEntry, ValueHash, TREE_DEPTH,
    },
    utils,
};

impl BlockOutputWithProofs {
    /// Verifies this output against the trusted old root hash of the tree and
    /// the applied instructions.
    ///
    /// # Panics
    ///
    /// Panics if the proof doesn't verify.
    pub fn verify_proofs(
        &self,
        hasher: &dyn HashTree,
        old_root_hash: ValueHash,
        instructions: &[(Key, TreeInstruction)],
    ) {
        assert_eq!(instructions.len(), self.logs.len());

        let mut root_hash = old_root_hash;
        for (op, &(key, instruction)) in self.logs.iter().zip(instructions) {
            assert!(op.merkle_path.len() <= TREE_DEPTH);
            if matches!(instruction, TreeInstruction::Read) {
                assert_eq!(op.root_hash, root_hash);
                assert!(op.base.is_read());
            } else {
                assert!(!op.base.is_read());
            }

            let (prev_leaf_index, leaf_index, prev_value) = match op.base {
                TreeLogEntry::Inserted { leaf_index } => (0, leaf_index, ValueHash::zero()),
                TreeLogEntry::Updated {
                    leaf_index,
                    previous_value,
                } => (leaf_index, leaf_index, previous_value),

                TreeLogEntry::Read { leaf_index, value } => (leaf_index, leaf_index, value),
                TreeLogEntry::ReadMissingKey => (0, 0, ValueHash::zero()),
            };

            let prev_hash =
                hasher.fold_merkle_path(&op.merkle_path, key, prev_value, prev_leaf_index);
            assert_eq!(prev_hash, root_hash);
            if let TreeInstruction::Write(value) = instruction {
                let next_hash = hasher.fold_merkle_path(&op.merkle_path, key, value, leaf_index);
                assert_eq!(next_hash, op.root_hash);
            }
            root_hash = op.root_hash;
        }
    }
}

impl TreeEntryWithProof {
    /// Verifies this proof.
    ///
    /// # Panics
    ///
    /// Panics if the proof doesn't verify.
    pub fn verify(&self, hasher: &dyn HashTree, key: Key, trusted_root_hash: ValueHash) {
        if self.base.leaf_index == 0 {
            assert!(
                self.base.value_hash.is_zero(),
                "Invalid missing value specification: leaf index is zero, but value is non-default"
            );
        }
        let root_hash = hasher.fold_merkle_path(
            &self.merkle_path,
            key,
            self.base.value_hash,
            self.base.leaf_index,
        );
        assert_eq!(root_hash, trusted_root_hash, "Root hash mismatch");
    }
}

/// Range digest in a Merkle tree allowing to compute its root hash based on the provided entries.
///
/// - The entries must be ordered by key. I.e., the first entry must have the numerically smallest key,
///   and the last entry must have the numerically greatest key among all provided entries.
/// - The first and the last entries must be provided together with a Merkle proof; other entries
///   do not need proofs.
/// - Any entry can be [empty](TreeEntry::is_empty()). I.e., there's no requirement to only
///   provide existing entries.
///
/// This construction is useful for verifying *Merkle range proofs*. Such a proof proves that
/// a certain key range in the Merkle tree contains the specified entries and no other entries.
///
/// # Implementation details
///
/// A streaming approach is used. `TreeRange` occupies `O(1)` RAM w.r.t. the number of entries.
/// `TreeRange` consists of `TREE_DEPTH = 256` hashes and a constant amount of other data.
//
// We keep a *left contour* of hashes, i.e., known hashes to the left of the last processed key.
// Initially, the left contour is a filtered Merkle path for the start entry; we only take into
// account left hashes in the path (ones for which the corresponding start key bit is 1), and
// discard right hashes.
//
// When a `TreeRange` is updated, we find the first diverging bit between the last processed key
// and the new key. (This bit is always 0 in the last processed key and 1 in the new key.)
//
// ```text
// ...
// diverging_level:          /                         \
// ...                       |  (only empty subtrees)  |
// TREE_DEPTH:           current_leaf              next_leaf
// ```
//
// We update the left contour by collapsing the last processed entry up to the diverging bit.
// When collapsing, we take advantage of the fact that all right hashes in the collapsed part
// of the Merkle path correspond to empty subtrees. We also clean all hashes in the left contour
// further down the tree; it's guaranteed that the next key will only have empty subtrees
// to the left of it until the diverging level.
//
// When we want to finalize a range, we update the left contour one final time, and then collapse
// the Merkle path for the final key all the way to the root hash. When doing this, we take
// right hashes from the provided path, and left hashes from the left contour (left hashes from
// the final entry Merkle path are discarded).
#[derive(Debug)]
pub struct TreeRangeDigest<'a> {
    hasher: HasherWithStats<'a>,
    current_leaf: LeafNode,
    left_contour: Box<[ValueHash; TREE_DEPTH]>,
}

impl<'a> TreeRangeDigest<'a> {
    /// Starts a new Merkle tree range.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub fn new(hasher: &'a dyn HashTree, start_key: Key, start_entry: &TreeEntryWithProof) -> Self {
        let full_path = hasher.extend_merkle_path(&start_entry.merkle_path);
        let left_contour = full_path.enumerate().map(|(depth, adjacent_hash)| {
            if start_key.bit(depth) {
                adjacent_hash // `adjacent_hash` is to the left of the `start_key`; take it
            } else {
                hasher.empty_subtree_hash(depth)
            }
        });
        let left_contour: Vec<_> = left_contour.collect();
        Self {
            hasher: HasherWithStats::new(hasher),
            current_leaf: LeafNode::new(
                start_key,
                start_entry.base.value_hash,
                start_entry.base.leaf_index,
            ),
            left_contour: left_contour.try_into().unwrap(),
            // ^ `unwrap()` is safe by construction; `left_contour` will always have necessary length
        }
    }

    /// Updates this digest with a new entry.
    ///
    /// # Panics
    ///
    /// Panics if the provided `key` is not greater than the previous key provided to this digest.
    pub fn update(&mut self, key: Key, entry: TreeEntry) {
        assert!(
            key > self.current_leaf.full_key,
            "Keys provided to a digest must be monotonically increasing"
        );

        let diverging_level = utils::find_diverging_bit(self.current_leaf.full_key, key) + 1;

        // Hash the current leaf up to the `diverging_level`, taking current `left_contour` into account.
        let mut hash = self
            .hasher
            .hash_leaf(&self.current_leaf.value_hash, self.current_leaf.leaf_index);
        for depth in 0..(TREE_DEPTH - diverging_level) {
            let empty_subtree_hash = self.hasher.empty_subtree_hash(depth);
            // Replace the left contour value with the default one.
            let left_hash = mem::replace(&mut self.left_contour[depth], empty_subtree_hash);

            hash = if self.current_leaf.full_key.bit(depth) {
                self.hasher.hash_branch(&left_hash, &hash)
            } else {
                // We don't take right contour into account, since by construction (because we iterate
                // over keys in ascending order) it's always empty.
                self.hasher.hash_branch(&hash, &empty_subtree_hash)
            };
        }
        // Record the computed hash.
        self.left_contour[TREE_DEPTH - diverging_level] = hash;
        self.current_leaf = LeafNode::new(key, entry.value_hash, entry.leaf_index);
    }

    /// Finalizes this digest and returns the root hash of the tree.
    ///
    /// # Panics
    ///
    /// Panics if the provided `final_key` is not greater than the previous key provided to this digest.
    pub fn finalize(mut self, final_key: Key, final_entry: &TreeEntryWithProof) -> ValueHash {
        self.update(final_key, final_entry.base);

        let full_path = self
            .hasher
            .inner
            .extend_merkle_path(&final_entry.merkle_path);
        let zipped_paths = self.left_contour.into_iter().zip(full_path);
        let mut hash = self
            .hasher
            .hash_leaf(&final_entry.base.value_hash, final_entry.base.leaf_index);
        for (depth, (left, right)) in zipped_paths.enumerate() {
            hash = if final_key.bit(depth) {
                self.hasher.hash_branch(&left, &hash)
            } else {
                self.hasher.hash_branch(&hash, &right)
            };
        }
        hash
    }
}

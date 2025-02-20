use std::{collections::BTreeMap, iter};

use anyhow::Context;
use zksync_basic_types::H256;

use crate::{types::Leaf, BatchOutput, HashTree, TreeEntry};

/// Operation on a Merkle tree entry used in [`BatchTreeProof`].
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub enum TreeOperation {
    /// Update of an existing entry.
    Update { index: u64 },
    /// Insertion of a new entry.
    Insert {
        /// Prev index before *batch* insertion (i.e., always points to an index existing before batch insertion).
        prev_index: u64,
    },
}

#[derive(Debug)]
pub struct IntermediateHash {
    pub value: H256,
    /// Level + index on level. Redundant and is only checked in tests.
    #[cfg(test)]
    pub location: (u8, u64),
}

/// Merkle proof of batch insertion into [`MerkleTree`](crate::MerkleTree).
///
/// # How it's verified
///
/// Assumes that the tree before insertion is correctly constructed (in particular, leaves are correctly linked via prev / next index).
/// Given that, proof verification is as follows:
///
/// 1. Check that all necessary leaves are present in `sorted_leaves`, and their keys match inserted / updated entries.
/// 2. Previous root hash of the tree is recreated using `sorted_leaves` and `hashes`.
/// 3. `sorted_leaves` are updated / extended as per inserted / updated entries.
/// 4. New root hash of the tree is recreated using updated `sorted_leaves` and (the same) `hashes`.
#[derive(Debug)]
pub struct BatchTreeProof {
    /// Performed tree operations. Correspond 1-to-1 to [`TreeEntry`]s.
    pub operations: Vec<TreeOperation>,
    /// Sorted leaves from the tree before insertion sufficient to prove it. Contains all updated leaves
    /// (incl. prev / next neighbors for the inserted leaves), and the last leaf in the tree if there are inserts.
    pub sorted_leaves: BTreeMap<u64, Leaf>,
    /// Hashes necessary and sufficient to restore previous and updated root hashes. Provided in the ascending `(depth, index_on_level)` order,
    /// where `depth == 0` are leaves, `depth == 1` are nodes aggregating leaf pairs etc.
    pub hashes: Vec<IntermediateHash>,
}

impl BatchTreeProof {
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            operations: vec![],
            sorted_leaves: BTreeMap::new(),
            hashes: vec![],
        }
    }

    /// Returns the new root hash of the tree on success.
    pub fn verify(
        mut self,
        hasher: &dyn HashTree,
        tree_depth: u8,
        prev_output: Option<BatchOutput>,
        entries: &[TreeEntry],
    ) -> anyhow::Result<H256> {
        let Some(prev_output) = prev_output else {
            return self.verify_for_empty_tree(hasher, tree_depth, entries);
        };

        anyhow::ensure!(
            self.operations.len() == entries.len(),
            "Unexpected operations length"
        );
        if let Some((max_idx, _)) = self.sorted_leaves.iter().next_back() {
            anyhow::ensure!(*max_idx < prev_output.leaf_count, "Index is too large");
        }

        let mut index_by_key: BTreeMap<_, _> = self
            .sorted_leaves
            .iter()
            .map(|(idx, leaf)| (leaf.key, *idx))
            .collect();

        let mut next_tree_index = prev_output.leaf_count;
        for (&operation, entry) in self.operations.iter().zip(entries) {
            match operation {
                TreeOperation::Update { index } => {
                    anyhow::ensure!(
                        index < prev_output.leaf_count,
                        "Updated non-existing index {index}"
                    );
                    let existing_leaf = self
                        .sorted_leaves
                        .get(&index)
                        .with_context(|| format!("Update for index {index} is not proven"))?;
                    anyhow::ensure!(
                        existing_leaf.key == entry.key,
                        "Update for index {index} has unexpected key"
                    );
                }
                TreeOperation::Insert { prev_index } => {
                    let prev_leaf = self.sorted_leaves.get(&prev_index).with_context(|| {
                        format!("prev leaf {prev_index} for {entry:?} is not proven")
                    })?;
                    anyhow::ensure!(prev_leaf.key < entry.key);

                    let old_next_index = prev_leaf.next_index;
                    let old_next_leaf =
                        self.sorted_leaves.get(&old_next_index).with_context(|| {
                            format!("old next leaf {old_next_index} for {entry:?} is not proven")
                        })?;
                    anyhow::ensure!(old_next_leaf.prev_index == prev_index);
                    anyhow::ensure!(entry.key < old_next_leaf.key);

                    index_by_key.insert(entry.key, next_tree_index);
                    next_tree_index += 1;
                }
            };
        }

        let restored_prev_hash = Self::zip_leaves(
            hasher,
            tree_depth,
            prev_output.leaf_count,
            self.sorted_leaves.iter().map(|(idx, leaf)| (*idx, leaf)),
            self.hashes.iter(),
        )?;
        anyhow::ensure!(
            restored_prev_hash == prev_output.root_hash,
            "Mismatch for previous root hash: prev_output={prev_output:?}, restored={restored_prev_hash:?}"
        );

        // Expand `leaves` with the newly inserted leaves and update the existing leaves.
        for (&operation, entry) in self.operations.iter().zip(entries) {
            match operation {
                TreeOperation::Update { index } => {
                    // We've checked the key correspondence already.
                    self.sorted_leaves.get_mut(&index).unwrap().value = entry.value;
                }
                TreeOperation::Insert { .. } => {
                    let mut it = index_by_key.range(entry.key..);
                    let (_, &this_index) = it.next().unwrap();
                    // `unwrap()`s below are safe: at least the pre-existing next index is greater, and the pre-existing prev index is lesser.
                    let (_, &next_index) = it.next().unwrap();
                    let (_, &prev_index) = index_by_key.range(..entry.key).next_back().unwrap();

                    self.sorted_leaves.insert(
                        this_index,
                        Leaf {
                            key: entry.key,
                            value: entry.value,
                            prev_index,
                            next_index,
                        },
                    );

                    // Prev / next leaves may be missing if they are inserted in the batch as well;
                    // in this case, prev / next index will be set correctly once the leaf is created.
                    if let Some(prev_leaf) = self.sorted_leaves.get_mut(&prev_index) {
                        prev_leaf.next_index = this_index;
                    }
                    if let Some(next_leaf) = self.sorted_leaves.get_mut(&next_index) {
                        next_leaf.prev_index = this_index;
                    }
                }
            }
        }

        Self::zip_leaves(
            hasher,
            tree_depth,
            next_tree_index,
            self.sorted_leaves.iter().map(|(idx, leaf)| (*idx, leaf)),
            self.hashes.iter(),
        )
    }

    fn verify_for_empty_tree(
        self,
        hasher: &dyn HashTree,
        tree_depth: u8,
        entries: &[TreeEntry],
    ) -> anyhow::Result<H256> {
        // The proof must be entirely empty since we can get all data from `entries`.
        anyhow::ensure!(self.sorted_leaves.is_empty());
        anyhow::ensure!(self.operations.is_empty());
        anyhow::ensure!(self.hashes.is_empty());

        let index_by_key: BTreeMap<_, _> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.key, i as u64 + 2))
            .collect();
        anyhow::ensure!(
            index_by_key.len() == entries.len(),
            "There are entries with duplicate keys"
        );

        let mut min_leaf_index = 1;
        let mut max_leaf_index = 0;
        let sorted_leaves = entries.iter().enumerate().map(|(i, entry)| {
            let this_index = i as u64 + 2;

            // The key itself is guaranteed to be the first yielded item, hence `skip(1)`.
            let mut it = index_by_key.range(entry.key..).skip(1);
            let next_index = it.next().map(|(_, idx)| *idx).unwrap_or_else(|| {
                max_leaf_index = this_index;
                1
            });
            let prev_index = index_by_key
                .range(..entry.key)
                .map(|(_, idx)| *idx)
                .next_back()
                .unwrap_or_else(|| {
                    min_leaf_index = this_index;
                    0
                });

            Leaf {
                key: entry.key,
                value: entry.value,
                prev_index,
                next_index,
            }
        });
        let sorted_leaves: Vec<_> = sorted_leaves.collect();

        let min_guard = Leaf {
            next_index: min_leaf_index,
            ..Leaf::MIN_GUARD
        };
        let max_guard = Leaf {
            prev_index: max_leaf_index,
            ..Leaf::MAX_GUARD
        };
        let leaves_with_guards = [(0, &min_guard), (1, &max_guard)]
            .into_iter()
            .chain((2..).zip(&sorted_leaves));

        Self::zip_leaves(
            hasher,
            tree_depth,
            2 + entries.len() as u64,
            leaves_with_guards,
            iter::empty(),
        )
    }

    fn zip_leaves<'a>(
        hasher: &dyn HashTree,
        tree_depth: u8,
        leaf_count: u64,
        sorted_leaves: impl Iterator<Item = (u64, &'a Leaf)>,
        mut hashes: impl Iterator<Item = &'a IntermediateHash>,
    ) -> anyhow::Result<H256> {
        let mut node_hashes: Vec<_> = sorted_leaves
            .map(|(idx, leaf)| (idx, hasher.hash_leaf(leaf)))
            .collect();
        let mut last_idx_on_level = leaf_count - 1;

        for depth in 0..tree_depth {
            let mut i = 0;
            let mut next_level_i = 0;
            while i < node_hashes.len() {
                let (current_idx, current_hash) = node_hashes[i];
                let next_level_hash = if current_idx % 2 == 1 {
                    // The hash to the left is missing; get it from `hashes`
                    i += 1;
                    let lhs = hashes.next().context("ran out of hashes")?;
                    #[cfg(test)]
                    assert_eq!(lhs.location, (depth, current_idx - 1));

                    hasher.hash_branch(&lhs.value, &current_hash)
                } else if let Some((_, next_hash)) = node_hashes
                    .get(i + 1)
                    .filter(|(next_idx, _)| *next_idx == current_idx + 1)
                {
                    i += 2;
                    hasher.hash_branch(&current_hash, next_hash)
                } else {
                    // The hash to the right is missing; get it from `hashes`, or set to the empty subtree hash if appropriate.
                    i += 1;
                    let rhs = if current_idx == last_idx_on_level {
                        hasher.empty_subtree_hash(depth)
                    } else {
                        let rhs = hashes.next().context("ran out of hashes")?;
                        #[cfg(test)]
                        assert_eq!(rhs.location, (depth, current_idx + 1));
                        rhs.value
                    };
                    hasher.hash_branch(&current_hash, &rhs)
                };

                node_hashes[next_level_i] = (current_idx / 2, next_level_hash);
                next_level_i += 1;
            }
            node_hashes.truncate(next_level_i);
            last_idx_on_level /= 2;
        }

        anyhow::ensure!(hashes.next().is_none(), "not all hashes consumed");

        Ok(node_hashes[0].1)
    }
}

#[cfg(test)]
mod tests {
    use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

    use super::*;

    #[test]
    fn insertion_proof_for_empty_tree() {
        let proof = BatchTreeProof::empty();
        let hash = proof.verify(&Blake2Hasher, 64, None, &[]).unwrap();
        assert_eq!(
            hash,
            "0x8a41011d351813c31088367deecc9b70677ecf15ffc24ee450045cdeaf447f63"
                .parse()
                .unwrap()
        );

        let proof = BatchTreeProof::empty();
        let entry = TreeEntry {
            key: H256::repeat_byte(0x01),
            value: H256::repeat_byte(0x10),
        };
        let hash = proof.verify(&Blake2Hasher, 64, None, &[entry]).unwrap();
        assert_eq!(
            hash,
            "0x91a1688c802dc607125d0b5e5ab4d95d89a4a4fb8cca71a122db6076cb70f8f3"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn basic_insertion_proof() {
        let proof = BatchTreeProof {
            operations: vec![TreeOperation::Insert { prev_index: 0 }],
            sorted_leaves: BTreeMap::from([(0, Leaf::MIN_GUARD), (1, Leaf::MAX_GUARD)]),
            hashes: vec![],
        };

        let empty_tree_output = BatchOutput {
            leaf_count: 2,
            root_hash: "0x8a41011d351813c31088367deecc9b70677ecf15ffc24ee450045cdeaf447f63"
                .parse()
                .unwrap(),
        };
        let new_tree_hash = proof
            .verify(
                &Blake2Hasher,
                64,
                Some(empty_tree_output),
                &[TreeEntry {
                    key: H256::repeat_byte(0x01),
                    value: H256::repeat_byte(0x10),
                }],
            )
            .unwrap();

        assert_eq!(
            new_tree_hash,
            "0x91a1688c802dc607125d0b5e5ab4d95d89a4a4fb8cca71a122db6076cb70f8f3"
                .parse()
                .unwrap()
        );
    }
}

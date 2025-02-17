use std::collections::BTreeMap;

use anyhow::Context;
use zksync_basic_types::H256;

use crate::{types::Leaf, HashTree, TreeEntry};

#[derive(Debug, Clone, Copy)]
pub enum TreeOperation {
    Update {
        index: u64,
    },
    Insert {
        /// Prev index before batch insertion.
        prev_index: u64,
    },
}

#[derive(Debug)]
pub struct BatchTreeProof {
    pub operations: Vec<TreeOperation>,
    pub sorted_leaves: BTreeMap<u64, Leaf>,
    pub hashes: Vec<H256>,
}

impl BatchTreeProof {
    pub fn verify(
        mut self,
        hasher: &dyn HashTree,
        tree_depth: u8,
        entries: &[TreeEntry],
        prev_leaf_count: u64,
        prev_hash: H256,
    ) -> anyhow::Result<H256> {
        anyhow::ensure!(
            self.operations.len() == entries.len(),
            "Unexpected operations length"
        );
        if let Some((max_idx, _)) = self.sorted_leaves.iter().next_back() {
            anyhow::ensure!(*max_idx < prev_leaf_count, "Index is too large");
        }

        let mut inserted_keys = BTreeMap::new();
        let mut next_tree_index = prev_leaf_count;
        for (&operation, entry) in self.operations.iter().zip(entries) {
            match operation {
                TreeOperation::Update { index } => {
                    anyhow::ensure!(
                        index < prev_leaf_count,
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

                    inserted_keys.insert(entry.key, next_tree_index);
                    next_tree_index += 1;
                }
            }
        }

        let actual_prev_hash = Self::zip_leaves(
            hasher,
            tree_depth,
            self.sorted_leaves.iter().map(|(idx, leaf)| (*idx, leaf)),
            self.hashes.iter().copied(),
        )?;
        anyhow::ensure!(actual_prev_hash == prev_hash, "Mismatch for previous hash");

        // Expand `leaves` with the newly inserted leaves and update the existing leaves.
        for (&operation, entry) in self.operations.iter().zip(entries) {
            match operation {
                TreeOperation::Update { index } => {
                    // We've checked the key correspondence already.
                    self.sorted_leaves.get_mut(&index).unwrap().value = entry.value;
                }
                TreeOperation::Insert { prev_index } => {
                    let mut it = inserted_keys.range(entry.key..);
                    // `unwrap()` is safe: the current leaf itself is always present.
                    let (_, &this_index) = it.next().unwrap();
                    let next_index = if let Some((_, local_idx)) = it.next() {
                        *local_idx
                    } else {
                        // Update the link for the existing leaf. Index access / `unwrap()` is safe since we've checked leaf existence before.
                        let old_next_index = self.sorted_leaves[&prev_index].next_index;
                        self.sorted_leaves
                            .get_mut(&old_next_index)
                            .unwrap()
                            .prev_index = this_index;
                        old_next_index
                    };

                    let prev_index = if let Some((_, local_idx)) =
                        inserted_keys.range(..entry.key).next()
                    {
                        *local_idx
                    } else {
                        self.sorted_leaves.get_mut(&prev_index).unwrap().next_index = this_index;
                        prev_index
                    };

                    self.sorted_leaves.insert(
                        this_index,
                        Leaf {
                            key: entry.key,
                            value: entry.value,
                            prev_index,
                            next_index,
                        },
                    );
                }
            }
        }

        Self::zip_leaves(
            hasher,
            tree_depth,
            self.sorted_leaves.iter().map(|(idx, leaf)| (*idx, leaf)),
            self.hashes.iter().copied(),
        )
    }

    fn zip_leaves<'a>(
        hasher: &dyn HashTree,
        tree_depth: u8,
        sorted_leaves: impl Iterator<Item = (u64, &'a Leaf)>,
        mut hashes: impl Iterator<Item = H256>,
    ) -> anyhow::Result<H256> {
        let mut node_hashes: Vec<_> = sorted_leaves
            .map(|(idx, leaf)| (idx, hasher.hash_leaf(leaf)))
            .collect();
        for depth in 0..tree_depth {
            let mut i = 0;
            let mut next_level_i = 0;
            while i < node_hashes.len() {
                next_level_i = i / 2;
                let (current_idx, current_hash) = node_hashes[i];
                let next_level_hash = if current_idx % 2 == 1 {
                    // The hash to the left is missing; get it from `hashes`
                    i += 1;
                    let lhs = hashes.next().context("ran out of hashes")?;
                    hasher.hash_branch(&lhs, &current_hash)
                } else if let Some((_, next_hash)) = node_hashes
                    .get(i + 1)
                    .filter(|(next_idx, _)| *next_idx == current_idx + 1)
                {
                    i += 2;
                    hasher.hash_branch(&current_hash, next_hash)
                } else {
                    // The hash to the right is missing; get it from `hashes`, or set to the empty subtree hash if it's missing.
                    i += 1;
                    let rhs = hashes
                        .next()
                        .unwrap_or_else(|| hasher.empty_subtree_hash(depth));
                    hasher.hash_branch(&current_hash, &rhs)
                };

                node_hashes[next_level_i] = (current_idx / 2, next_level_hash);
            }
            node_hashes.truncate(next_level_i + 1);
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
    fn basic_insertion_proof() {
        let proof = BatchTreeProof {
            operations: vec![TreeOperation::Insert { prev_index: 0 }],
            sorted_leaves: BTreeMap::from([(0, Leaf::MIN_GUARD), (1, Leaf::MAX_GUARD)]),
            hashes: vec![],
        };

        let empty_tree_hash: H256 =
            "0x8a41011d351813c31088367deecc9b70677ecf15ffc24ee450045cdeaf447f63"
                .parse()
                .unwrap();
        let new_tree_hash = proof
            .verify(
                &Blake2Hasher,
                64,
                &[TreeEntry {
                    key: H256::repeat_byte(0x01),
                    value: H256::repeat_byte(0x10),
                }],
                2,
                empty_tree_hash,
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

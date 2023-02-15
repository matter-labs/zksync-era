use crate::iter_ext::IteratorExt;
use crate::types::{NodeEntry, TreeKey};
use crate::{Bytes, TreeError};
use core::iter;
use itertools::Itertools;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use std::collections::HashMap;
use zksync_config::constants::ROOT_TREE_DEPTH;
use zksync_crypto::hasher::Hasher;

/// Represents set of prepared updates to be applied to the given tree in batch.
/// To calculate actual patch, use [crate::UpdatesMap::calculate].
pub struct UpdatesBatch {
    updates: HashMap<
        // key affected by given storage update on respective tree level
        TreeKey,
        Vec<Update>,
    >,
}

/// Set of patches combined into one.
/// Each element represents changes from a single slot update.
pub type TreePatch = Vec<Vec<(TreeKey, NodeEntry)>>;

#[derive(Clone, Debug)]
pub struct Update {
    // operation index in a batch
    index: usize,
    // hashes of neighbour nodes on the path from root to the leaf
    uncles: Vec<Vec<u8>>,
    // all branch nodes that changed due to given update
    // empty initially; populated level-by-level during path calculate phasE
    changes: Vec<(TreeKey, NodeEntry)>,
}

impl Update {
    pub fn new(index: usize, uncles: Vec<Vec<u8>>, key: TreeKey) -> Self {
        let mut update = Self {
            index,
            uncles,
            changes: Vec::with_capacity(ROOT_TREE_DEPTH + 1),
        };
        update.changes.push((
            key,
            NodeEntry::Leaf {
                hash: update.uncles.pop().unwrap(),
            },
        ));
        update
    }
}

impl UpdatesBatch {
    /// Instantiates new set of batch updates.
    pub(crate) fn new(updates: HashMap<TreeKey, Vec<Update>>) -> Self {
        Self { updates }
    }

    /// Calculates new set of Merkle Trees produced by applying map of updates to the current tree.
    /// This calculation is parallelized over operations - all trees will be calculated in parallel.
    ///
    /// Memory and time: O(M * log2(N)), where
    /// - N - count of all leaf nodes (basically 2 in power of depth)
    /// - M - count of updates being applied
    pub fn calculate<H>(self, hasher: H) -> Result<TreePatch, TreeError>
    where
        H: Hasher<Bytes> + Send + Sync,
    {
        let res_map = (0..ROOT_TREE_DEPTH).fold(self.updates, |cur_lvl_updates_map, _| {
            // Calculate next level map based on current in parallel
            cur_lvl_updates_map
                .into_iter()
                .into_grouping_map_by(|(key, _)| key >> 1)
                .fold((None, None), |acc, _, item| {
                    if item.0 % 2 == 1.into() {
                        (acc.0, Some(item.1))
                    } else {
                        (Some(item.1), acc.1)
                    }
                })
                // Parallel by vertex key family
                .into_par_iter()
                .map(|(next_idx, (left_updates, right_updates))| {
                    let left_updates = left_updates
                        .into_iter()
                        .flat_map(|items| iter::repeat(false).zip(items));

                    let right_updates = right_updates
                        .into_iter()
                        .flat_map(|items| iter::repeat(true).zip(items));

                    let merged_ops: Vec<_> = left_updates
                        .merge_join_with_max_predecessor(
                            right_updates,
                            |(_, left), (_, right)| left.index.cmp(&right.index),
                            |(_, update)| update.changes.last().map(|(_, node)| node).cloned(),
                        )
                        .collect();

                    let ops_iter = merged_ops
                        // Parallel by operation index
                        .into_par_iter()
                        .map(|((odd, mut update), nei)| {
                            let Update {
                                uncles, changes, ..
                            } = &mut update;

                            let current_hash = changes
                                .last()
                                .map(|(_, node)| node.hash().to_vec())
                                .unwrap();

                            let sibling_hash = uncles.pop().unwrap();
                            let nei_hash = nei
                                .flatten()
                                .map(NodeEntry::into_hash)
                                .unwrap_or(sibling_hash);

                            // Hash current node with its neighbor
                            let (left_hash, right_hash) = if odd {
                                (nei_hash, current_hash)
                            } else {
                                (current_hash, nei_hash)
                            };

                            let branch = NodeEntry::Branch {
                                hash: hasher.compress(&left_hash, &right_hash),
                                left_hash,
                                right_hash,
                            };

                            changes.push((next_idx, branch));

                            update
                        });

                    (next_idx, ops_iter.collect())
                })
                .collect()
        });

        // Transforms map of leaf keys into an iterator of Merkle paths which produces
        // items sorted by operation index in increasing order.
        let patch = res_map
            .into_iter()
            .flat_map(|(_, updates)| updates.into_iter().map(|update| update.changes))
            .collect();

        Ok(patch)
    }
}

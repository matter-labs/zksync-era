//! Logic specific to the full tree operation mode, in which it produces Merkle proofs
//! for each operation.
//!
//! # How it works
//!
//! As with the proof-less [`Self::extend()`], we start by loading all relevant nodes
//! from the database and determining the parent node position for each key in `instructions`.
//!
//! A naive implementation would then apply `instructions` one by one, creating a proof
//! for each of instructions. This, however, is quite slow (determined mainly by the hash
//! operations that need to be performed to create Merkle proofs). So, we parallelize
//! the process by splitting the instructions by the first key nibble (i.e., into 16 key groups)
//! and working on each group in parallel. Each group of instructions is *mostly* independent,
//! since it's mostly applied to a separate subtree of the original Merkle tree
//! with root at level 4 (= 1 nibble). Thus, the patch sets and Merkle proofs
//! produced by each group are mostly disjoint; they intersect only at the root node level.
//!
//! ## Merging Merkle proofs
//!
//! The proofs produced by different groups only intersect at levels 0..4. This can be dealt with
//! as follows:
//!
//! - Produce partial Merkle proofs for levels 4.. (rather than full proofs for levels 0..)
//!   when working in groups. The root hash for each of the proofs will actually be the
//!   *subtree* root hash, and Merkle proofs would have at most 252 `ValueHash`es.
//! - Recombine the proofs in the original `instructions` order. For each write instruction,
//!   update the corresponding child reference hash in the root node to equal
//!   the (subtree) root hash from the proof, and recompute the root hash of the root node.
//!   Then, extend the Merkle proof with upper 4 `ValueHash`es based on the root node.
//!
//! This approach only works if the root is an [`InternalNode`]. Fortunately, we can always
//! transform the root to an `InternalNode` and then transform it back if necessary.
//!
//! ## Merging patch sets
//!
//! `WorkingPatchSet`s produced by different groups are disjoint except for the root node.
//! We ignore the root node in these sets anyway; the final root node is produced by applying
//! logs with proofs as described above. Thus, we can merge patch sets just by merging
//! their nibbles–node entries.

use rayon::prelude::*;

use crate::{
    hasher::{HasherWithStats, MerklePath},
    metrics::{HashingStats, TreeUpdaterStats, BLOCK_TIMINGS, GENERAL_METRICS},
    storage::{Database, NewLeafData, PatchSet, SortedKeys, Storage, TreeUpdater},
    types::{
        BlockOutputWithProofs, InternalNode, Key, Nibbles, Node, TreeInstruction, TreeLogEntry,
        TreeLogEntryWithProof, ValueHash,
    },
    utils::merge_by_index,
};

/// Number of subtrees used for parallel computations.
pub(super) const SUBTREE_COUNT: usize = 16;
/// 0-based tree level at which subtree roots are located.
const SUBTREE_ROOT_LEVEL: usize = 4;

impl TreeUpdater {
    fn extend_precomputed(
        &mut self,
        hasher: &mut HasherWithStats<'_>,
        first_nibble: u8,
        instructions: Vec<InstructionWithPrecomputes>,
    ) -> Vec<(usize, TreeLogEntryWithProof<MerklePath>)> {
        let mut logs = Vec::with_capacity(instructions.len());
        let root_nibbles = Nibbles::single(first_nibble);
        let mut root_hash = match self.patch_set.get(&root_nibbles) {
            Some(node) => node.hash(hasher, SUBTREE_ROOT_LEVEL),
            None => hasher.empty_subtree_hash(SUBTREE_ROOT_LEVEL),
        };

        for instruction in instructions {
            let InstructionWithPrecomputes {
                index,
                instruction,
                parent_nibbles,
            } = instruction;

            let log = match instruction {
                TreeInstruction::Write(entry) => {
                    let (log, leaf_data) = self.insert(entry, &parent_nibbles);
                    let (new_root_hash, merkle_path) = self.update_node_hashes(hasher, &leaf_data);
                    root_hash = new_root_hash;
                    TreeLogEntryWithProof {
                        base: log,
                        merkle_path,
                        root_hash,
                    }
                }
                TreeInstruction::Read(key) => {
                    let (log, merkle_path) = self.prove(hasher, key, &parent_nibbles);
                    TreeLogEntryWithProof {
                        base: log,
                        merkle_path,
                        root_hash,
                    }
                }
            };
            logs.push((index, log));
        }
        logs
    }

    /// Updates hashes for the leaves inserted or updated in the tree together with all ancestor
    /// internal nodes. Returns the new root hash of the tree and the Merkle path
    /// for the inserted / updated key.
    fn update_node_hashes(
        &mut self,
        hasher: &mut HasherWithStats<'_>,
        leaf_data: &NewLeafData,
    ) -> (ValueHash, MerklePath) {
        if let Some((nibbles, leaf)) = leaf_data.adjacent_leaf {
            let (parent_nibbles, last_nibble) = nibbles.split_last().unwrap();
            let leaf_level = nibbles.nibble_count() * 4;
            debug_assert!(leaf_level >= SUBTREE_ROOT_LEVEL);
            // ^ Because we've ensured an internal root node, all inserted leaves have at least
            // 1 nibble.
            let node_hash = leaf.hash(hasher, leaf_level);
            self.patch_set
                .child_ref_mut(&parent_nibbles, last_nibble)
                .unwrap()
                .hash = node_hash;
            // ^ This only works because the parent node has been just created; in the general case,
            // mutating `ChildRef.hash` invalidates `InternalNodeCache`.
        }

        let mut nibbles = leaf_data.nibbles;
        let leaf_level = nibbles.nibble_count() * 4;
        debug_assert!(leaf_level >= SUBTREE_ROOT_LEVEL);
        let mut node_hash = leaf_data.leaf.hash(hasher, leaf_level);
        let mut merkle_path = MerklePath::new(leaf_level);
        while let Some((parent_nibbles, last_nibble)) = nibbles.split_last() {
            if parent_nibbles.nibble_count() == 0 {
                break;
            }

            let parent = self.patch_set.get_mut(&parent_nibbles);
            let Some(Node::Internal(parent)) = parent else {
                unreachable!()
            };
            let parent_level = parent_nibbles.nibble_count() * 4;
            let mut updater = parent.updater(hasher, parent_level, last_nibble);
            node_hash = updater.update_child_hash(node_hash);
            updater.extend_merkle_path(&mut merkle_path);
            nibbles = parent_nibbles;
        }

        (node_hash, merkle_path)
    }

    /// Proves the existence or absence of a key in the tree.
    pub(super) fn prove(
        &mut self,
        hasher: &mut HasherWithStats<'_>,
        key: Key,
        parent_nibbles: &Nibbles,
    ) -> (TreeLogEntry, MerklePath) {
        let (leaf, merkle_path) =
            self.patch_set
                .create_proof(hasher, key, parent_nibbles, SUBTREE_ROOT_LEVEL / 4);
        let operation = leaf.map_or(TreeLogEntry::ReadMissingKey, |leaf| {
            TreeLogEntry::read(leaf.leaf_index, leaf.value_hash)
        });

        if matches!(operation, TreeLogEntry::ReadMissingKey) {
            self.metrics.missing_key_reads += 1;
        } else {
            self.metrics.key_reads += 1;
        }
        (operation, merkle_path)
    }

    fn split(self) -> [Self; SUBTREE_COUNT] {
        self.patch_set.split().map(|patch_set| Self {
            metrics: TreeUpdaterStats::default(),
            patch_set,
        })
    }

    fn merge(mut self, other: Self) -> Self {
        self.patch_set.merge(other.patch_set);
        self.metrics += other.metrics;
        self
    }

    /// Sequentially applies `logs` produced by parallelized tree traversal updating the root node
    /// using log data. Finalizes Merkle paths in each log.
    fn finalize_logs(
        &mut self,
        hasher: &mut HasherWithStats<'_>,
        mut root: InternalNode,
        logs: Vec<(usize, TreeLogEntryWithProof<MerklePath>)>,
    ) -> Vec<TreeLogEntryWithProof> {
        let version = self.patch_set.root_version();
        let mut root_hash = root.hash(hasher, 0);

        // Check the kind of each of subtrees. This is used later to ensure the correct
        // `ChildRef.is_leaf` values in the root node.
        let mut is_leaf_by_subtree = [false; SUBTREE_COUNT];
        for (subtree_idx, is_leaf) in is_leaf_by_subtree.iter_mut().enumerate() {
            let nibble = u8::try_from(subtree_idx).unwrap();
            let child = self.patch_set.get(&Nibbles::single(nibble));
            *is_leaf = matches!(child, Some(Node::Leaf(_)));
        }

        let logs = logs.into_iter().map(|(subtree_idx, mut log)| {
            let nibble = u8::try_from(subtree_idx).unwrap();
            let mut updater = root.updater(hasher, 0, nibble);
            if !log.base.is_read() {
                updater.ensure_child_ref(version, is_leaf_by_subtree[subtree_idx]);
                root_hash = updater.update_child_hash(log.root_hash);
            }
            updater.extend_merkle_path(&mut log.merkle_path);

            TreeLogEntryWithProof {
                base: log.base,
                merkle_path: log.merkle_path.into_inner(),
                root_hash,
            }
        });
        let logs = logs.collect();

        if root.child_count() == 0 {
            // We cannot save the empty internal root node because it'll fail deserialization
            // checks later. By construction, the patch set is guaranteed to be valid (namely empty)
            // after removal.
            self.patch_set.take_root();
        } else {
            self.set_root_node(root.into());
        }
        logs
    }
}

impl<DB: Database + ?Sized> Storage<'_, DB> {
    pub fn extend_with_proofs(
        mut self,
        instructions: Vec<TreeInstruction>,
    ) -> (BlockOutputWithProofs, PatchSet) {
        let load_nodes_latency = BLOCK_TIMINGS.load_nodes.start();
        let sorted_keys = SortedKeys::new(instructions.iter().map(TreeInstruction::key));
        let parent_nibbles = self.updater.load_ancestors(&sorted_keys, self.db);
        load_nodes_latency.observe();

        let instruction_parts = InstructionWithPrecomputes::split(instructions, parent_nibbles);
        let initial_root = self.updater.patch_set.ensure_internal_root_node();
        let initial_metrics = self.updater.metrics;
        let storage_parts = self.updater.split();

        let hashing_stats = HashingStats::default();

        let extend_patch_latency = BLOCK_TIMINGS.extend_patch.start();
        // `into_par_iter()` below uses `rayon` to parallelize tree traversal and proof generation.
        let (storage_parts, logs): (Vec<_>, Vec<_>) = storage_parts
            .into_par_iter()
            .zip_eq(instruction_parts)
            .enumerate()
            .map_init(
                || self.hasher.with_stats(&hashing_stats),
                |hasher, (i, (mut storage, instructions))| {
                    let first_nibble = u8::try_from(i).unwrap();
                    let logs = storage.extend_precomputed(hasher, first_nibble, instructions);
                    (storage, logs)
                },
            )
            .unzip();
        extend_patch_latency.observe();

        let finalize_patch_latency = BLOCK_TIMINGS.finalize_patch.start();
        self.updater = storage_parts
            .into_iter()
            .reduce(TreeUpdater::merge)
            .unwrap();
        // ^ `unwrap()` is safe: `storage_parts` is non-empty
        self.updater.metrics += initial_metrics;

        let logs = merge_by_index(logs);
        let mut hasher = self.hasher.with_stats(&hashing_stats);
        let output_with_proofs = self.finalize_with_proofs(&mut hasher, initial_root, logs);
        finalize_patch_latency.observe();
        drop(hasher);
        hashing_stats.report();

        output_with_proofs
    }

    fn finalize_with_proofs(
        mut self,
        hasher: &mut HasherWithStats<'_>,
        root: InternalNode,
        logs: Vec<(usize, TreeLogEntryWithProof<MerklePath>)>,
    ) -> (BlockOutputWithProofs, PatchSet) {
        self.leaf_count += self.updater.metrics.new_leaves;
        tracing::debug!(
            "Finished updating tree; total leaf count: {}, stats: {:?}",
            self.leaf_count,
            self.updater.metrics
        );
        let logs = self.updater.finalize_logs(hasher, root, logs);
        self.updater.metrics.report();

        let patch = self
            .updater
            .patch_set
            .finalize_without_hashing(self.manifest, self.leaf_count);
        let block_output = BlockOutputWithProofs {
            logs,
            leaf_count: self.leaf_count,
        };
        GENERAL_METRICS.leaf_count.set(self.leaf_count);

        (block_output, patch)
    }
}

/// [`TreeInstruction`] together with precomputed data necessary to efficiently parallelize
/// Merkle tree traversal.
#[derive(Debug)]
struct InstructionWithPrecomputes {
    /// 0-based index of the instruction.
    index: usize,
    instruction: TreeInstruction,
    /// Nibbles for the parent node computed by [`Storage::load_ancestors()`].
    parent_nibbles: Nibbles,
}

impl InstructionWithPrecomputes {
    /// Creates groups of instructions to be used during parallelized tree traversal.
    fn split(
        instructions: Vec<TreeInstruction>,
        parent_nibbles: Vec<Nibbles>,
    ) -> [Vec<Self>; SUBTREE_COUNT] {
        const EMPTY_VEC: Vec<InstructionWithPrecomputes> = Vec::new();
        // ^ Need to extract this to a constant to be usable as an array initializer.

        let mut parts = [EMPTY_VEC; SUBTREE_COUNT];
        let it = instructions.into_iter().zip(parent_nibbles);
        for (index, (instruction, parent_nibbles)) in it.enumerate() {
            let first_nibble = Nibbles::nibble(&instruction.key(), 0);
            let part = &mut parts[first_nibble as usize];
            part.push(Self {
                index,
                instruction,
                parent_nibbles,
            });
        }
        parts
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;
    use crate::types::Root;

    fn byte_key(byte: u8) -> Key {
        Key::from_little_endian(&[byte; 32])
    }

    #[test]
    fn sorting_keys() {
        let keys = [4, 1, 5, 2, 3].map(byte_key);
        let sorted_keys = SortedKeys::new(keys.into_iter());
        assert_eq!(sorted_keys.0, [1, 3, 4, 0, 2].map(|i| (i, keys[i])));
    }

    #[test]
    fn proofs_for_empty_storage() {
        let db = PatchSet::default();
        let storage = Storage::new(&db, &(), 0, true);
        let instructions = vec![
            TreeInstruction::Read(byte_key(1)),
            TreeInstruction::Read(byte_key(2)),
            TreeInstruction::Read(byte_key(0xff)),
        ];
        let (block_output, patch) = storage.extend_with_proofs(instructions);
        assert_eq!(block_output.leaf_count, 0);
        let all_misses = block_output
            .logs
            .iter()
            .all(|log| matches!(log.base, TreeLogEntry::ReadMissingKey));
        assert!(all_misses);

        assert_matches!(patch.patches_by_version[&0].root, Some(Root::Empty));
    }
}

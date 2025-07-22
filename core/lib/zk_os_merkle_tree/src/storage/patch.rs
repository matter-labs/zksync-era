use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    marker::PhantomData,
    mem, ops,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use zksync_basic_types::H256;

use super::{AsEntry, Database, InsertedKeyEntry, PartialPatchSet, PatchSet};
use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    hasher::{BatchTreeProof, IntermediateHash, InternalHashes, TreeOperation},
    leaf_nibbles, max_nibbles_for_internal_node, max_node_children,
    metrics::{BatchProofStage, LoadStage, METRICS},
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root, TreeTags},
    BatchOutput, DeserializeError, HashTree, MerkleTree, TreeEntry, TreeParams,
};

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct InsertedLeaf {
    pub(super) leaf: Leaf,
    /// Prev index before the batch insertion. `None` if the prev leaf is inserted in the same batch.
    pub(super) prev_index: Option<u64>,
}

/// Information about an atomic tree update.
#[must_use = "Should be applied to a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct TreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    pub(crate) updates: Vec<(u64, H256)>,
    /// Inserted leaves together with an index of the previous leaf (which also may be among the inserted leaves).
    pub(crate) inserts: Vec<InsertedLeaf>,
    operations: Vec<TreeOperation>,
    read_operations: Vec<TreeOperation>,
    /// Indices of leaves that are loaded just for a batch proof for read operations. These leaves
    /// are removed from a [`WorkingPatchSet`] after the proof is created.
    pub(crate) readonly_leaf_indices: Vec<u64>,
    pub(crate) missing_reads_count: usize,
}

impl TreeUpdate {
    pub(crate) fn for_empty_tree<E: AsEntry>(entries: &[E]) -> anyhow::Result<Self> {
        let mut sorted_new_leaves = BTreeMap::from([
            (
                H256::zero(),
                InsertedKeyEntry {
                    index: 0,
                    inserted_at: 0,
                },
            ),
            (
                H256::repeat_byte(0xff),
                InsertedKeyEntry {
                    index: 1,
                    inserted_at: 0,
                },
            ),
        ]);

        for (i, entry) in entries.iter().enumerate() {
            let index = i as u64 + 2;
            entry.check_index(index)?;
            sorted_new_leaves.insert(
                entry.as_entry().key,
                InsertedKeyEntry {
                    index,
                    inserted_at: 0,
                },
            );
        }

        anyhow::ensure!(
            sorted_new_leaves.len() == entries.len() + 2,
            "Attempting to insert duplicate keys into a tree; please deduplicate keys on the caller side"
        );

        let mut inserts = Vec::with_capacity(entries.len() + 2);
        for entry in [&TreeEntry::MIN_GUARD, &TreeEntry::MAX_GUARD]
            .into_iter()
            .chain(entries.iter().map(E::as_entry))
        {
            let next_range = (ops::Bound::Excluded(entry.key), ops::Bound::Unbounded);
            let next_index = match sorted_new_leaves.range(next_range).next() {
                Some((_, next_entry)) => next_entry.index,
                None => {
                    assert_eq!(entry.key, H256::repeat_byte(0xff));
                    1
                }
            };

            let leaf = Leaf {
                key: entry.key,
                value: entry.value,
                next_index,
            };
            inserts.push(InsertedLeaf {
                leaf,
                prev_index: None,
            });
        }

        Ok(Self {
            version: 0,
            sorted_new_leaves,
            updates: vec![],
            inserts,
            // Since the tree is empty, we expect a completely empty `BatchTreeProof` incl. `operations`; see its validation logic.
            // This makes the proof occupy less space and doesn't require to specify bogus indices for `TreeOperation::Insert`.
            operations: vec![],
            read_operations: vec![],
            readonly_leaf_indices: vec![],
            missing_reads_count: 0,
        })
    }

    pub(crate) fn take_operations(&mut self) -> Vec<TreeOperation> {
        mem::take(&mut self.operations)
    }

    pub(crate) fn take_read_operations(&mut self) -> Vec<TreeOperation> {
        mem::take(&mut self.read_operations)
    }
}

#[must_use = "Should be finalized with a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct FinalTreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
}

impl PartialPatchSet {
    /// Updates ancestor's `ChildRef` version for all loaded internal nodes. This should be called before adding new leaves
    /// to the tree; it works because the loaded leaves are exactly the leaves for which ancestor versions must be updated.
    fn update_ancestor_versions<P: TreeParams>(&mut self, version: u64) {
        let mut indices: Vec<_> = self.leaves.keys().copied().collect();
        indices.sort_unstable();

        for internal_level in self.internal.iter_mut().rev() {
            let mut prev_index = None;
            indices = indices
                .into_iter()
                .filter_map(|idx| {
                    let parent_idx = idx >> P::INTERNAL_NODE_DEPTH;
                    let parent = internal_level.get_mut(&parent_idx).unwrap();
                    parent
                        .child_mut((idx % u64::from(max_node_children::<P>())) as usize)
                        .version = version;

                    if prev_index == Some(parent_idx) {
                        None
                    } else {
                        prev_index = Some(parent_idx);
                        Some(parent_idx)
                    }
                })
                .collect();
        }
    }

    fn remove_readonly_nodes(&mut self, updated_version: u64) -> usize {
        // We always retain the root node (i.e., the only node in `self.internal[0]`), even if the tree hasn't changed.
        let mut removed_node_count = 0;
        for internal_level in &mut self.internal[1..] {
            internal_level.retain(|_, node| {
                let should_retain = node
                    .children
                    .iter()
                    .any(|child_ref| child_ref.version == updated_version);
                removed_node_count += usize::from(!should_retain);
                should_retain
            })
        }
        removed_node_count
    }
}

#[derive(Debug)]
pub(crate) struct WorkingPatchSet<P> {
    inner: PartialPatchSet,
    _params: PhantomData<P>,
}

impl<P: TreeParams> WorkingPatchSet<P> {
    pub(crate) fn empty() -> Self {
        Self::new(Root {
            leaf_count: 0,
            root_node: InternalNode::empty(),
        })
    }

    fn new(root: Root) -> Self {
        let mut internal = vec![HashMap::new(); max_nibbles_for_internal_node::<P>() as usize + 1];
        internal[0].insert(0, root.root_node);

        Self {
            inner: PartialPatchSet {
                leaf_count: root.leaf_count,
                internal,
                leaves: HashMap::new(),
            },
            _params: PhantomData,
        }
    }

    #[cfg(test)]
    pub(super) fn inner(&self) -> &PartialPatchSet {
        &self.inner
    }

    /// `leaf_indices` must be sorted.
    fn load_nodes(
        &mut self,
        db: &impl Database,
        leaf_indices: impl Iterator<Item = u64> + Clone,
    ) -> anyhow::Result<()> {
        let this = &mut self.inner;
        for nibble_count in 1..=leaf_nibbles::<P>() {
            let bit_shift = (leaf_nibbles::<P>() - nibble_count) * P::INTERNAL_NODE_DEPTH;

            let mut prev_index_on_level = None;
            let parent_level = &this.internal[usize::from(nibble_count) - 1];

            let requested_keys = leaf_indices.clone().filter_map(|idx| {
                let index_on_level = idx >> bit_shift;
                if prev_index_on_level == Some(index_on_level) {
                    None
                } else {
                    prev_index_on_level = Some(index_on_level);
                    let parent_idx = index_on_level >> P::INTERNAL_NODE_DEPTH;
                    let parent = &parent_level[&parent_idx];
                    let child_idx = index_on_level % u64::from(max_node_children::<P>());
                    let this_ref = parent.child_ref(child_idx as usize);
                    let requested_key = NodeKey {
                        version: this_ref.version,
                        nibble_count,
                        index_on_level,
                    };
                    Some((index_on_level, requested_key))
                }
            });
            let (indices, requested_keys): (Vec<_>, Vec<_>) = requested_keys.unzip();
            let loaded_nodes = db.try_nodes(&requested_keys)?;

            if nibble_count == leaf_nibbles::<P>() {
                this.leaves = loaded_nodes
                    .into_iter()
                    .zip(indices)
                    .map(|(node, idx)| {
                        (
                            idx,
                            match node {
                                Node::Leaf(leaf) => leaf,
                                Node::Internal(_) => unreachable!(),
                            },
                        )
                    })
                    .collect();
            } else {
                this.internal[usize::from(nibble_count)] = loaded_nodes
                    .into_iter()
                    .zip(indices)
                    .map(|(node, idx)| {
                        (
                            idx,
                            match node {
                                Node::Internal(node) => node,
                                Node::Leaf(_) => unreachable!(),
                            },
                        )
                    })
                    .collect();
            }
        }

        Ok(())
    }

    pub(crate) fn loaded_leaves_count(&self) -> usize {
        self.inner.leaves.len()
    }

    pub(crate) fn loaded_internal_nodes_count(&self) -> usize {
        self.inner.total_internal_nodes()
    }

    pub(crate) fn create_batch_proof(
        &self,
        hasher: &P::Hasher,
        operations: Vec<TreeOperation>,
        read_operations: Vec<TreeOperation>,
    ) -> BatchTreeProof {
        let sorted_leaves: BTreeMap<_, _> = self
            .inner
            .leaves
            .iter()
            .map(|(idx, leaf)| (*idx, *leaf))
            .collect();
        BatchTreeProof {
            operations,
            read_operations,
            hashes: self.collect_hashes(sorted_leaves.keys().copied().collect(), hasher),
            sorted_leaves,
        }
    }

    /// Provides necessary and sufficient hashes for a [`BatchTreeProof`]. Should be called before any modifying operations;
    /// by design, leaves loaded for a batch update are exactly leaves included into a `BatchTreeProof`.
    ///
    /// `leaf_indices` is the sorted list of all loaded leaves.
    fn collect_hashes(&self, leaf_indices: Vec<u64>, hasher: &P::Hasher) -> Vec<IntermediateHash> {
        let mut indices_on_level = leaf_indices;
        if indices_on_level.is_empty() {
            return vec![];
        }

        let this = &self.inner;
        let mut hashes = vec![];
        // Should not underflow because `indices_on_level` is non-empty.
        let mut last_idx_on_level = this.leaf_count - 1;

        let mut internal_hashes = None;
        let mut internal_node_levels = this.internal.iter().rev();
        let mut hash_latency = Duration::ZERO;
        let mut traverse_latency = Duration::ZERO;

        // The logic below essentially repeats `BatchTreeProof::zip_leaves()`, only instead of taking provided hashes,
        // they are put in `hashes`.
        for depth in 0..P::TREE_DEPTH {
            let depth_in_internal_node = depth % P::INTERNAL_NODE_DEPTH;
            if depth_in_internal_node == 0 {
                // Initialize / update `internal_hashes`. Computing *all* internal hashes may be somewhat inefficient,
                // but since it's parallelized, it doesn't look like a major concern.
                let level = internal_node_levels
                    .next()
                    .expect("run out of internal node levels");
                let started_at = Instant::now();
                internal_hashes = Some(InternalHashes::new::<P>(level, hasher, depth));
                hash_latency += started_at.elapsed();
            }

            let started_at = Instant::now();
            // `unwrap()` is safe; `internal_hashes` is initialized on the first loop iteration
            let internal_hashes = internal_hashes.as_ref().unwrap();

            let mut i = 0;
            let mut next_level_i = 0;
            while i < indices_on_level.len() {
                let current_idx = indices_on_level[i];
                if current_idx % 2 == 1 {
                    // The hash to the left is missing; get it from `hashes`
                    i += 1;
                    hashes.push(IntermediateHash {
                        value: internal_hashes.get(depth_in_internal_node, current_idx - 1),
                        #[cfg(test)]
                        location: (depth, current_idx - 1),
                    });
                } else if indices_on_level
                    .get(i + 1)
                    .is_some_and(|&next_idx| next_idx == current_idx + 1)
                {
                    i += 2;
                    // Don't get the hash, it'll be available locally.
                } else {
                    // The hash to the right is missing; get it from `hashes`, or set to the empty subtree hash.
                    i += 1;
                    if current_idx < last_idx_on_level {
                        hashes.push(IntermediateHash {
                            value: internal_hashes.get(depth_in_internal_node, current_idx + 1),
                            #[cfg(test)]
                            location: (depth, current_idx + 1),
                        });
                    }
                };

                indices_on_level[next_level_i] = current_idx / 2;
                next_level_i += 1;
            }
            indices_on_level.truncate(next_level_i);
            last_idx_on_level /= 2;
            traverse_latency += started_at.elapsed();
        }

        METRICS.batch_proof_latency[&BatchProofStage::Hashing].observe(hash_latency);
        METRICS.batch_proof_latency[&BatchProofStage::Traversal].observe(traverse_latency);
        tracing::debug!(
            ?hash_latency,
            ?traverse_latency,
            "collected hashes for batch proof"
        );
        hashes
    }

    pub(crate) fn update(&mut self, update: TreeUpdate) -> FinalTreeUpdate {
        let this = &mut self.inner;
        let version = update.version;

        // Remove readonly leaves first, so that we don't update bogus ancestors. If there are no read ops
        // (or if there's no batch proof created), `update.readonly_leaf_indices` is empty, so we don't pay the overhead of doing this.
        let readonly_leaf_indices_len = update.readonly_leaf_indices.len();
        for idx in update.readonly_leaf_indices {
            this.leaves.remove(&idx);
        }

        // Update ancestor versions based on the remaining leaves.
        this.update_ancestor_versions::<P>(version);
        if readonly_leaf_indices_len > 0 {
            // Filter out all internal nodes that were not updated (= don't have updated child refs).
            let removed_node_count = this.remove_readonly_nodes(version);
            METRICS.readonly_leaves.observe(readonly_leaf_indices_len);
            METRICS.readonly_internal_nodes.observe(removed_node_count);
            tracing::debug!(
                leaves = readonly_leaf_indices_len,
                internal_nodes = removed_node_count,
                "removed readonly nodes from patch"
            );
        }

        for (idx, value) in update.updates {
            this.leaves.get_mut(&idx).unwrap().value = value;
        }

        if !update.inserts.is_empty() {
            let first_new_idx = this.leaf_count;
            // Cannot underflow because `update.inserts.len() >= 1`
            let new_indexes = first_new_idx..=(first_new_idx + update.inserts.len() as u64 - 1);

            // Update the next index pointer for the neighbor.
            for (idx, new_leaf) in new_indexes.clone().zip(&update.inserts) {
                if let Some(prev_idx) = new_leaf.prev_index {
                    this.leaves.get_mut(&prev_idx).unwrap().next_index = idx;
                }
            }

            let inserts = update.inserts.into_iter().map(|leaf| leaf.leaf);
            this.leaves.extend(new_indexes.clone().zip(inserts));
            this.leaf_count = *new_indexes.end() + 1;

            // Add / update internal nodes.
            for (i, internal_level) in this.internal.iter_mut().enumerate() {
                let nibble_count = i as u8;
                let child_depth =
                    (max_nibbles_for_internal_node::<P>() - nibble_count) * P::INTERNAL_NODE_DEPTH;
                let first_index_on_level =
                    (new_indexes.start() >> child_depth) / u64::from(max_node_children::<P>());
                let last_child_index = new_indexes.end() >> child_depth;
                let last_index_on_level = last_child_index / u64::from(max_node_children::<P>());

                // Only `first_index_on_level` may exist already; all others are necessarily new.
                let mut start_idx = first_index_on_level;
                if let Some(parent) = internal_level.get_mut(&first_index_on_level) {
                    let expected_len = if last_index_on_level == first_index_on_level {
                        (last_child_index % u64::from(max_node_children::<P>())) as usize + 1
                    } else {
                        max_node_children::<P>().into()
                    };
                    parent.ensure_len(expected_len, version);
                    start_idx += 1;
                }

                let new_nodes = (start_idx..=last_index_on_level).map(|idx| {
                    let expected_len = if idx == last_index_on_level {
                        (last_child_index % u64::from(max_node_children::<P>())) as usize + 1
                    } else {
                        max_node_children::<P>().into()
                    };
                    (idx, InternalNode::new(expected_len, version))
                });
                internal_level.extend(new_nodes);
            }
        }

        FinalTreeUpdate {
            version,
            sorted_new_leaves: update.sorted_new_leaves,
        }
    }

    pub(crate) fn finalize(
        self,
        hasher: &P::Hasher,
        update: FinalTreeUpdate,
    ) -> (PatchSet, BatchOutput) {
        use rayon::prelude::*;

        let mut this = self.inner;
        let mut hashes: Vec<_> = this
            .leaves
            .par_iter()
            .map(|(idx, leaf)| (*idx, hasher.hash_leaf(leaf)))
            .collect();

        for (nibble_depth, internal_level) in this.internal.iter_mut().rev().enumerate() {
            for (idx, hash) in hashes {
                // The parent node must exist by construction.
                internal_level
                    .get_mut(&(idx >> P::INTERNAL_NODE_DEPTH))
                    .unwrap()
                    .child_mut((idx % u64::from(max_node_children::<P>())) as usize)
                    .hash = hash;
            }

            let depth = nibble_depth as u8 * P::INTERNAL_NODE_DEPTH;
            hashes = internal_level
                .par_iter()
                .map(|(idx, node)| (*idx, node.hash::<P>(hasher, depth)))
                .collect();
        }

        assert_eq!(hashes.len(), 1);
        let (root_idx, root_hash) = hashes[0];
        assert_eq!(root_idx, 0);
        let output = BatchOutput {
            leaf_count: this.leaf_count,
            root_hash,
        };

        // Release excessive capacity occupied by the partial patch set. We'll never modify it inside a `PatchSet`.
        this.leaves.shrink_to_fit();
        for internal_level in &mut this.internal {
            internal_level.shrink_to_fit();
        }

        let patch = PatchSet {
            manifest: Manifest {
                version_count: update.version + 1,
                tags: TreeTags::for_params::<P>(hasher),
            },
            patches_by_version: HashMap::from([(update.version, this)]),
            sorted_new_leaves: update.sorted_new_leaves,
        };
        (patch, output)
    }
}

impl<DB: Database, P: TreeParams> MerkleTree<DB, P> {
    /// Loads data for processing the specified entries into a patch set.
    #[tracing::instrument(
        level = "debug",
        skip(self, entries, read_keys),
        fields(entries.len = entries.len(), read_keys.len = read_keys.len())
    )]
    pub(crate) fn create_patch<E: AsEntry>(
        &self,
        latest_version: u64,
        entries: &[E],
        read_keys: &[H256],
    ) -> anyhow::Result<(WorkingPatchSet<P>, TreeUpdate)> {
        let root = self.db.try_root(latest_version)?.ok_or_else(|| {
            DeserializeError::from(DeserializeErrorKind::MissingNode)
                .with_context(DeserializeContext::Node(NodeKey::root(latest_version)))
        })?;
        let touched_keys: Vec<_> = entries
            .iter()
            .map(|entry| entry.as_entry().key)
            .chain(read_keys.iter().copied())
            .collect();

        let key_lookup_latency = METRICS.load_nodes_latency[&LoadStage::KeyLookup].start();
        let lookup = self
            .db
            .indices(latest_version, &touched_keys)
            .context("failed loading indices")?;
        let elapsed = key_lookup_latency.observe();
        tracing::debug!(?elapsed, "loaded lookup info");

        // Collect all distinct indices that need to be loaded.
        let mut sorted_new_leaves = BTreeMap::new();
        let mut new_index = root.leaf_count;
        let mut distinct_indices = BTreeSet::new();
        for (lookup, entry) in lookup.iter().zip(entries) {
            match lookup {
                KeyLookup::Existing(idx) => {
                    entry.check_index(*idx)?;
                    distinct_indices.insert(*idx);
                }
                KeyLookup::Missing {
                    prev_key_and_index,
                    next_key_and_index,
                } => {
                    entry.check_index(new_index)?;
                    sorted_new_leaves.insert(
                        entry.as_entry().key,
                        InsertedKeyEntry {
                            index: new_index,
                            inserted_at: latest_version + 1,
                        },
                    );
                    new_index += 1;

                    distinct_indices.extend([prev_key_and_index.1, next_key_and_index.1]);
                }
            }
        }

        if !sorted_new_leaves.is_empty() {
            // Need to load the latest existing leaf and its ancestors so that new ancestors can be correctly
            // inserted for the new leaves.
            distinct_indices.insert(root.leaf_count - 1);
        }

        let mut read_operations = Vec::with_capacity(read_keys.len());
        let mut missing_reads_count = 0;
        let mut readonly_leaf_indices = vec![];
        for lookup in &lookup[entries.len()..] {
            let read_op = match lookup {
                KeyLookup::Existing(idx) => {
                    if distinct_indices.insert(*idx) {
                        readonly_leaf_indices.push(*idx);
                    }
                    TreeOperation::Hit { index: *idx }
                }
                KeyLookup::Missing {
                    prev_key_and_index: (_, prev_idx),
                    next_key_and_index: (_, next_idx),
                } => {
                    missing_reads_count += 1;
                    if distinct_indices.insert(*prev_idx) {
                        readonly_leaf_indices.push(*prev_idx);
                    }
                    if distinct_indices.insert(*next_idx) {
                        readonly_leaf_indices.push(*next_idx);
                    }

                    TreeOperation::Miss {
                        prev_index: *prev_idx,
                    }
                }
            };
            read_operations.push(read_op);
        }

        let tree_nodes_latency = METRICS.load_nodes_latency[&LoadStage::TreeNodes].start();
        let mut patch = WorkingPatchSet::new(root);
        patch.load_nodes(&self.db, distinct_indices.iter().copied())?;
        let elapsed = tree_nodes_latency.observe();
        tracing::debug!(
            ?elapsed,
            distinct_indices.len = distinct_indices.len(),
            "loaded tree nodes"
        );

        let mut updates = Vec::with_capacity(entries.len() - sorted_new_leaves.len());
        let mut inserts = Vec::with_capacity(sorted_new_leaves.len());
        let mut operations = Vec::with_capacity(entries.len());
        for (entry, lookup) in entries.iter().zip(&lookup) {
            let entry = entry.as_entry();
            match lookup {
                &KeyLookup::Existing(idx) => {
                    updates.push((idx, entry.value));
                    operations.push(TreeOperation::Hit { index: idx });
                }

                KeyLookup::Missing {
                    prev_key_and_index,
                    next_key_and_index,
                } => {
                    // Adjust previous / next indices according to the data inserted in the same batch.
                    let prev_index = prev_key_and_index.1;
                    operations.push(TreeOperation::Miss { prev_index });

                    let mut prev_index = Some(prev_index);
                    if let Some((&local_prev_key, _)) =
                        sorted_new_leaves.range(..entry.key).next_back()
                    {
                        if local_prev_key > prev_key_and_index.0 {
                            prev_index = None;
                        }
                    }

                    let mut next_index = next_key_and_index.1;
                    let next_range = (ops::Bound::Excluded(entry.key), ops::Bound::Unbounded);
                    if let Some((&local_next_key, inserted)) =
                        sorted_new_leaves.range(next_range).next()
                    {
                        if local_next_key < next_key_and_index.0 {
                            next_index = inserted.index;
                        }
                    }

                    let leaf = Leaf {
                        key: entry.key,
                        value: entry.value,
                        next_index,
                    };
                    inserts.push(InsertedLeaf { leaf, prev_index });
                }
            }
        }

        anyhow::ensure!(
            sorted_new_leaves.len() == inserts.len(),
            "Attempting to insert duplicate keys into a tree; please deduplicate keys on the caller side"
        );
        // We don't check for duplicate updates since they don't lead to logical errors, they're just inefficient

        Ok((
            patch,
            TreeUpdate {
                version: latest_version + 1,
                sorted_new_leaves,
                updates,
                inserts,
                operations,
                read_operations,
                readonly_leaf_indices,
                missing_reads_count,
            },
        ))
    }
}

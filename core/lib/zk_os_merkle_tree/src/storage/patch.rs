use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    marker::PhantomData,
    ops,
    time::Instant,
};

use anyhow::Context as _;
use zksync_basic_types::H256;

use super::{Database, InsertedKeyEntry, PartialPatchSet, PatchSet};
use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    leaf_nibbles, max_nibbles_for_internal_node, max_node_children,
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root, TreeTags},
    DeserializeError, HashTree, MerkleTree, TreeEntry, TreeParams,
};

/// Information about an atomic tree update.
#[must_use = "Should be applied to a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct TreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    pub(crate) updates: Vec<(u64, H256)>,
    pub(crate) inserts: Vec<Leaf>,
}

impl TreeUpdate {
    pub(crate) fn for_empty_tree(entries: &[TreeEntry]) -> anyhow::Result<Self> {
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
        sorted_new_leaves.extend(entries.iter().enumerate().map(|(i, entry)| {
            (
                entry.key,
                InsertedKeyEntry {
                    index: i as u64 + 2,
                    inserted_at: 0,
                },
            )
        }));

        anyhow::ensure!(
            sorted_new_leaves.len() == entries.len() + 2,
            "Attempting to insert duplicate keys into a tree; please deduplicate keys on the caller side"
        );

        let mut inserts = Vec::with_capacity(entries.len() + 2);
        for entry in [&TreeEntry::MIN_GUARD, &TreeEntry::MAX_GUARD]
            .into_iter()
            .chain(entries)
        {
            let prev_index = match sorted_new_leaves.range(..entry.key).next_back() {
                Some((_, prev_entry)) => prev_entry.index,
                None => {
                    assert_eq!(entry.key, H256::zero());
                    0
                }
            };

            let next_range = (ops::Bound::Excluded(entry.key), ops::Bound::Unbounded);
            let next_index = match sorted_new_leaves.range(next_range).next() {
                Some((_, next_entry)) => next_entry.index,
                None => {
                    assert_eq!(entry.key, H256::repeat_byte(0xff));
                    1
                }
            };

            inserts.push(Leaf {
                key: entry.key,
                value: entry.value,
                prev_index,
                next_index,
            });
        }

        Ok(Self {
            version: 0,
            sorted_new_leaves,
            updates: vec![],
            inserts,
        })
    }
}

#[must_use = "Should be finalized with a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct FinalTreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
}

impl PartialPatchSet {
    fn update_ancestor_versions<P: TreeParams>(&mut self, leaf_idx: u64, version: u64) {
        let mut idx = leaf_idx;
        for internal_level in self.internal.iter_mut().rev() {
            let parent = internal_level
                .get_mut(&(idx >> P::INTERNAL_NODE_DEPTH))
                .unwrap();
            parent
                .child_mut((idx % u64::from(max_node_children::<P>())) as usize)
                .version = version;
            idx >>= P::INTERNAL_NODE_DEPTH;
        }
        self.root.root_node.child_mut(idx as usize).version = version;
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
        Self {
            inner: PartialPatchSet {
                root,
                internal: vec![HashMap::new(); max_nibbles_for_internal_node::<P>() as usize],
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
            let parent_level = usize::from(nibble_count)
                .checked_sub(2)
                .map(|i| &this.internal[i]);

            let requested_keys = leaf_indices.clone().filter_map(|idx| {
                let index_on_level = idx >> bit_shift;
                if prev_index_on_level == Some(index_on_level) {
                    None
                } else {
                    prev_index_on_level = Some(index_on_level);

                    let parent = if let Some(parent_level) = parent_level {
                        let parent_idx = index_on_level >> P::INTERNAL_NODE_DEPTH;
                        &parent_level[&parent_idx]
                    } else {
                        // nibble_count == 1, the parent is the root node
                        &this.root.root_node
                    };
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
                this.internal[nibble_count as usize - 1] = loaded_nodes
                    .into_iter()
                    .zip(indices)
                    .map(|(node, idx)| {
                        (
                            idx,
                            match node {
                                Node::Internal(node) => node,
                                Node::Leaf(_) => {
                                    dbg!(
                                        nibble_count,
                                        P::TREE_DEPTH,
                                        P::INTERNAL_NODE_DEPTH,
                                        leaf_nibbles::<P>()
                                    );
                                    unreachable!()
                                }
                            },
                        )
                    })
                    .collect();
            }
        }

        Ok(())
    }

    pub(crate) fn total_internal_nodes(&self) -> usize {
        self.inner.internal.iter().map(HashMap::len).sum()
    }

    pub(crate) fn update(&mut self, update: TreeUpdate) -> FinalTreeUpdate {
        let this = &mut self.inner;
        let version = update.version;
        for (idx, value) in update.updates {
            this.leaves.get_mut(&idx).unwrap().value = value;
            this.update_ancestor_versions::<P>(idx, version);
        }

        if !update.inserts.is_empty() {
            let first_new_idx = this.root.leaf_count;
            // Cannot underflow because `update.inserts.len() >= 1`
            let new_indexes = first_new_idx..=(first_new_idx + update.inserts.len() as u64 - 1);

            // Update prev / next index pointers for neighbors.
            for (idx, new_leaf) in new_indexes.clone().zip(&update.inserts) {
                // Prev / next leaf may also be new, in which case, we'll insert it with the correct prev / next pointers,
                // so we don't need to do anything here.
                let prev_idx = new_leaf.prev_index;
                if let Some(prev_leaf) = this.leaves.get_mut(&prev_idx) {
                    prev_leaf.next_index = idx;
                    this.update_ancestor_versions::<P>(prev_idx, version);
                }

                let next_idx = new_leaf.next_index;
                if let Some(next_leaf) = this.leaves.get_mut(&next_idx) {
                    next_leaf.prev_index = idx;
                    this.update_ancestor_versions::<P>(next_idx, version);
                }
            }

            this.leaves.extend(new_indexes.clone().zip(update.inserts));
            this.root.leaf_count = *new_indexes.end() + 1;

            // Add / update internal nodes.
            for (i, internal_level) in this.internal.iter_mut().enumerate() {
                let nibble_count = i as u8 + 1;
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

            let child_depth = max_nibbles_for_internal_node::<P>() * P::INTERNAL_NODE_DEPTH;
            let last_child_index = new_indexes.end() >> child_depth;
            assert!(last_child_index < u64::from(max_node_children::<P>()));

            this.root
                .root_node
                .ensure_len(last_child_index as usize + 1, version);

            this.update_ancestor_versions::<P>(first_new_idx, version);
        }

        FinalTreeUpdate {
            version,
            sorted_new_leaves: update.sorted_new_leaves,
        }
    }

    pub(crate) fn finalize(self, hasher: &P::Hasher, update: FinalTreeUpdate) -> (PatchSet, H256) {
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

        for (idx, hash) in hashes {
            assert!(idx < u64::from(max_node_children::<P>()));
            this.root
                .root_node
                .child_mut((idx % u64::from(max_node_children::<P>())) as usize)
                .hash = hash;
        }
        let root_hash = this.root.root_node.hash::<P>(
            hasher,
            max_nibbles_for_internal_node::<P>() * P::INTERNAL_NODE_DEPTH,
        );

        let patch = PatchSet {
            manifest: Manifest {
                version_count: update.version + 1,
                tags: TreeTags::for_params::<P>(hasher),
            },
            patches_by_version: HashMap::from([(update.version, this)]),
            sorted_new_leaves: update.sorted_new_leaves,
        };
        (patch, root_hash)
    }
}

impl<DB: Database, P: TreeParams> MerkleTree<DB, P> {
    /// Loads data for processing the specified entries into a patch set.
    #[tracing::instrument(
        level = "debug",
        skip(self, entries),
        fields(entries.len = entries.len())
    )]
    pub(crate) fn create_patch(
        &self,
        latest_version: u64,
        entries: &[TreeEntry],
    ) -> anyhow::Result<(WorkingPatchSet<P>, TreeUpdate)> {
        let root = self.db.try_root(latest_version)?.ok_or_else(|| {
            DeserializeError::from(DeserializeErrorKind::MissingNode)
                .with_context(DeserializeContext::Node(NodeKey::root(latest_version)))
        })?;
        let keys: Vec<_> = entries.iter().map(|entry| entry.key).collect();

        let started_at = Instant::now();
        let lookup = self
            .db
            .indices(u64::MAX, &keys)
            .context("failed loading indices")?;
        tracing::debug!(elapsed = ?started_at.elapsed(), "loaded lookup info");

        // Collect all distinct indices that need to be loaded.
        let mut sorted_new_leaves = BTreeMap::new();
        let mut new_index = root.leaf_count;
        let distinct_indices =
            lookup
                .iter()
                .zip(entries)
                .flat_map(|(lookup, entry)| match lookup {
                    KeyLookup::Existing(idx) => [*idx, *idx],
                    KeyLookup::Missing {
                        prev_key_and_index,
                        next_key_and_index,
                    } => {
                        sorted_new_leaves.insert(
                            entry.key,
                            InsertedKeyEntry {
                                index: new_index,
                                inserted_at: latest_version + 1,
                            },
                        );
                        new_index += 1;

                        [prev_key_and_index.1, next_key_and_index.1]
                    }
                });
        let mut distinct_indices: BTreeSet<_> = distinct_indices.collect();
        if !sorted_new_leaves.is_empty() {
            // Need to load the latest existing leaf and its ancestors so that new ancestors can be correctly
            // inserted for the new leaves.
            distinct_indices.insert(root.leaf_count - 1);
        }

        let started_at = Instant::now();
        let mut patch = WorkingPatchSet::new(root);
        patch.load_nodes(&self.db, distinct_indices.iter().copied())?;
        tracing::debug!(
            elapsed = ?started_at.elapsed(),
            distinct_indices.len = distinct_indices.len(),
            "loaded nodes"
        );

        let mut updates = Vec::with_capacity(entries.len() - sorted_new_leaves.len());
        let mut inserts = Vec::with_capacity(sorted_new_leaves.len());
        for (entry, lookup) in entries.iter().zip(lookup) {
            match lookup {
                KeyLookup::Existing(idx) => {
                    updates.push((idx, entry.value));
                }

                KeyLookup::Missing {
                    prev_key_and_index,
                    next_key_and_index,
                } => {
                    // Adjust previous / next indices according to the data inserted in the same batch.
                    let mut prev_index = prev_key_and_index.1;
                    if let Some((&local_prev_key, inserted)) =
                        sorted_new_leaves.range(..entry.key).next_back()
                    {
                        if local_prev_key > prev_key_and_index.0 {
                            prev_index = inserted.index;
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

                    inserts.push(Leaf {
                        key: entry.key,
                        value: entry.value,
                        prev_index,
                        next_index,
                    });
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
            },
        ))
    }
}

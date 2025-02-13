use std::{
    array,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops,
};

use anyhow::Context as _;
use zksync_basic_types::H256;

use super::{Database, InsertedKeyEntry, PatchSet};
use crate::{
    errors::{DeserializeContext, DeserializeErrorKind},
    types::{InternalNode, KeyLookup, Leaf, Manifest, Node, NodeKey, Root},
    DeserializeError, HashTree, MerkleTree, TreeEntry,
};

/// Information about an atomic tree update.
#[must_use = "Should be applied to a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct TreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
    pub(super) updates: Vec<(u64, H256)>,
    pub(super) inserts: Vec<Leaf>,
}

impl TreeUpdate {
    pub(crate) fn for_empty_tree(entries: &[TreeEntry]) -> Self {
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

        Self {
            version: 0,
            sorted_new_leaves,
            updates: vec![],
            inserts,
        }
    }
}

#[must_use = "Should be finalized with a `PartialPatchSet`"]
#[derive(Debug)]
pub(crate) struct FinalTreeUpdate {
    pub(super) version: u64,
    pub(super) sorted_new_leaves: BTreeMap<H256, InsertedKeyEntry>,
}

#[derive(Debug)]
pub(crate) struct PartialPatchSet {
    pub(super) root: Root,
    // FIXME: maybe, a wrapper around `Vec<(_, _)>` would be more efficient?
    /// Offset by 1 (i.e., `internal[0]` corresponds to 1 nibble).
    pub(super) internal: [HashMap<u64, InternalNode>; InternalNode::MAX_NIBBLES as usize],
    /// Sorted by the index.
    pub(super) leaves: HashMap<u64, Leaf>,
}

impl PartialPatchSet {
    pub(crate) fn empty() -> Self {
        Self::new(Root {
            leaf_count: 0,
            root_node: InternalNode::empty(),
        })
    }

    fn new(root: Root) -> Self {
        Self {
            root,
            internal: array::from_fn(|_| HashMap::new()),
            leaves: HashMap::new(),
        }
    }

    /// `leaf_indices` must be sorted.
    fn load_nodes(
        &mut self,
        db: &impl Database,
        leaf_indices: impl Iterator<Item = u64> + Clone,
    ) -> anyhow::Result<()> {
        for nibble_count in 1..=Leaf::NIBBLES {
            let bit_shift = (Leaf::NIBBLES - nibble_count) * 4;

            let mut prev_index_on_level = None;
            let parent_level = usize::from(nibble_count)
                .checked_sub(2)
                .map(|i| &self.internal[i]);

            let requested_keys = leaf_indices.clone().filter_map(|idx| {
                let index_on_level = idx >> bit_shift;
                if prev_index_on_level == Some(index_on_level) {
                    None
                } else {
                    prev_index_on_level = Some(index_on_level);

                    let parent = if let Some(parent_level) = parent_level {
                        let parent_idx = index_on_level >> 4;
                        &parent_level[&parent_idx]
                    } else {
                        // nibble_count == 1, the parent is the root node
                        &self.root.root_node
                    };
                    let this_ref = parent.child_ref((index_on_level % 16) as usize);
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

            if nibble_count == Leaf::NIBBLES {
                self.leaves = loaded_nodes
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
                self.internal[nibble_count as usize - 1] = loaded_nodes
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

    pub(super) fn node(&self, nibble_count: u8, index_on_level: u64) -> Option<Node> {
        Some(if nibble_count == Leaf::NIBBLES {
            (*self.leaves.get(&index_on_level)?).into()
        } else {
            let level = &self.internal[usize::from(nibble_count) - 1];
            level.get(&index_on_level)?.clone().into()
        })
    }

    fn update_ancestor_versions(&mut self, leaf_idx: u64, version: u64) {
        let mut idx = leaf_idx;
        for internal_level in self.internal.iter_mut().rev() {
            let parent = internal_level.get_mut(&(idx >> 4)).unwrap();
            parent.child_mut((idx % 16) as usize).version = version;
            idx >>= 4;
        }
        self.root.root_node.child_mut(idx as usize).version = version;
    }

    pub(crate) fn update(&mut self, update: TreeUpdate) -> FinalTreeUpdate {
        let version = update.version;
        for (idx, value) in update.updates {
            self.leaves.get_mut(&idx).unwrap().value = value;
            self.update_ancestor_versions(idx, version);
        }

        if !update.inserts.is_empty() {
            let new_idx = self.root.leaf_count;
            // Cannot underflow because `update.inserts.len() >= 1`
            let mut new_indexes = new_idx..=(new_idx + update.inserts.len() as u64 - 1);

            // Update prev / next index pointers for neighbors.
            for (idx, new_leaf) in new_indexes.clone().zip(&update.inserts) {
                // Prev / next leaf may also be new, in which case, we'll insert it with the correct prev / next pointers,
                // so we don't need to do anything here.
                let prev_idx = new_leaf.prev_index;
                if let Some(prev_leaf) = self.leaves.get_mut(&prev_idx) {
                    prev_leaf.next_index = idx;
                    self.update_ancestor_versions(prev_idx, version);
                }

                let next_idx = new_leaf.next_index;
                if let Some(next_leaf) = self.leaves.get_mut(&next_idx) {
                    next_leaf.prev_index = idx;
                    self.update_ancestor_versions(next_idx, version);
                }
            }

            self.leaves.extend(new_indexes.clone().zip(update.inserts));
            self.root.leaf_count = *new_indexes.end() + 1;

            // Add / update internal nodes.
            for internal_level in self.internal.iter_mut().rev() {
                let mut len = new_indexes.end() - new_indexes.start() + 1;
                let parent_indexes = (new_indexes.start() >> 4)..=(new_indexes.end() >> 4);

                // Only the first of `parent_indexes` may exist already; all others are necessarily new.
                if let Some(parent) = internal_level.get_mut(parent_indexes.start()) {
                    let expected_len = (new_indexes.start() % 16 + len).min(16);
                    parent.ensure_len(expected_len as usize, version);
                } else {
                    internal_level.insert(
                        *parent_indexes.start(),
                        InternalNode::new(len.min(16) as usize, version),
                    );
                }
                len = len.saturating_sub(16);

                for parent_idx in parent_indexes.clone().skip(1) {
                    internal_level
                        .insert(parent_idx, InternalNode::new(len.min(16) as usize, version));
                    len = len.saturating_sub(16);
                }

                new_indexes = parent_indexes;
            }

            assert!(*new_indexes.end() < 16);
            self.root
                .root_node
                .ensure_len(*new_indexes.end() as usize + 1, version);
        }

        FinalTreeUpdate {
            version,
            sorted_new_leaves: update.sorted_new_leaves,
        }
    }

    pub(crate) fn finalize(mut self, hasher: &dyn HashTree, update: FinalTreeUpdate) -> PatchSet {
        use rayon::prelude::*;

        let mut hashes: Vec<_> = self
            .leaves
            .par_iter()
            .map(|(idx, leaf)| (*idx, hasher.hash_leaf(leaf)))
            .collect();

        for (nibble_depth, internal_level) in self.internal.iter_mut().rev().enumerate() {
            for (idx, hash) in hashes {
                // The parent node must exist by construction.
                internal_level
                    .get_mut(&(idx >> 4))
                    .unwrap()
                    .child_mut((idx % 16) as usize)
                    .hash = hash;
            }

            let depth = nibble_depth as u8 * 4;
            hashes = internal_level
                .par_iter()
                .map(|(idx, node)| (*idx, node.hash(hasher, depth)))
                .collect();
        }

        for (idx, hash) in hashes {
            assert!(idx < 16);
            self.root.root_node.child_mut((idx % 16) as usize).hash = hash;
        }

        PatchSet {
            manifest: Manifest {
                version_count: update.version + 1,
            },
            patches_by_version: HashMap::from([(update.version, self)]),
            sorted_new_leaves: update.sorted_new_leaves,
        }
    }
}

impl<DB: Database, H: HashTree> MerkleTree<DB, H> {
    /// Loads data for processing the specified entries into a patch set.
    pub(crate) fn create_patch(
        &self,
        latest_version: u64,
        entries: &[TreeEntry],
    ) -> anyhow::Result<(PartialPatchSet, TreeUpdate)> {
        let root = self.db.try_root(latest_version)?.ok_or_else(|| {
            DeserializeError::from(DeserializeErrorKind::MissingNode)
                .with_context(DeserializeContext::Node(NodeKey::root(latest_version)))
        })?;
        let keys: Vec<_> = entries.iter().map(|entry| entry.key).collect();
        let lookup = self
            .db
            .indices(u64::MAX, &keys)
            .context("failed loading indices")?;

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
        let distinct_indices: BTreeSet<_> = distinct_indices.collect();

        let mut patch = PartialPatchSet::new(root);
        patch.load_nodes(&self.db, distinct_indices.iter().copied())?;

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

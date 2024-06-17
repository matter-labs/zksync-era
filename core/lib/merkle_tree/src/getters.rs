//! Getters for the Merkle tree.

use crate::{
    hasher::HasherWithStats,
    recovery::MerkleTreeRecovery,
    storage::{LoadAncestorsResult, SortedKeys, WorkingPatchSet},
    types::{Nibbles, Node, ProfiledTreeOperation, TreeEntry, TreeEntryWithProof},
    Database, HashTree, Key, MerkleTree, NoVersionError, PruneDatabase, ValueHash,
};

impl<DB: Database, H: HashTree> MerkleTree<DB, H> {
    /// Reads entries with the specified keys from the tree. The entries are returned in the same order
    /// as requested. If a certain key is not present in the tree, the corresponding returned entry
    /// will be [empty](TreeEntry::is_empty()).
    ///
    /// # Errors
    ///
    /// Returns an error if the tree `version` is missing.
    pub fn entries(
        &self,
        version: u64,
        leaf_keys: &[Key],
    ) -> Result<Vec<TreeEntry>, NoVersionError> {
        let _profiling_guard = self.db.start_profiling(ProfiledTreeOperation::GetEntries);
        load_and_transform_entries(&self.db, version, leaf_keys, extract_entry)
    }

    /// Reads entries together with Merkle proofs with the specified keys from the tree. The entries are returned
    /// in the same order as requested.
    ///
    /// # Errors
    ///
    /// Returns an error if the tree `version` is missing.
    pub fn entries_with_proofs(
        &self,
        version: u64,
        leaf_keys: &[Key],
    ) -> Result<Vec<TreeEntryWithProof>, NoVersionError> {
        let mut hasher = HasherWithStats::new(&self.hasher);
        let _profiling_guard = self
            .db
            .start_profiling(ProfiledTreeOperation::GetEntriesWithProofs);
        load_and_transform_entries(
            &self.db,
            version,
            leaf_keys,
            |patch_set, &leaf_key, longest_prefix| {
                let (leaf, merkle_path) =
                    patch_set.create_proof(&mut hasher, leaf_key, longest_prefix, 0);
                let value = leaf
                    .as_ref()
                    .map_or_else(ValueHash::zero, |leaf| leaf.value_hash);
                TreeEntry {
                    key: leaf_key,
                    value,
                    leaf_index: leaf.map_or(0, |leaf| leaf.leaf_index),
                }
                .with_merkle_path(merkle_path.into_inner())
            },
        )
    }
}

fn load_and_transform_entries<T>(
    db: &impl Database,
    version: u64,
    leaf_keys: &[Key],
    mut transform: impl FnMut(&mut WorkingPatchSet, &Key, &Nibbles) -> T,
) -> Result<Vec<T>, NoVersionError> {
    let root = db.root(version).ok_or_else(|| {
        let manifest = db.manifest().unwrap_or_default();
        NoVersionError {
            missing_version: version,
            version_count: manifest.version_count,
        }
    })?;
    let sorted_keys = SortedKeys::new(leaf_keys.iter().copied());
    let mut patch_set = WorkingPatchSet::new(version, root);
    let LoadAncestorsResult {
        longest_prefixes, ..
    } = patch_set.load_ancestors(&sorted_keys, db);

    Ok(leaf_keys
        .iter()
        .zip(&longest_prefixes)
        .map(|(leaf_key, longest_prefix)| transform(&mut patch_set, leaf_key, longest_prefix))
        .collect())
}

fn extract_entry(
    patch_set: &mut WorkingPatchSet,
    leaf_key: &Key,
    longest_prefix: &Nibbles,
) -> TreeEntry {
    let node = patch_set.get(longest_prefix);
    match node {
        Some(Node::Leaf(leaf)) if &leaf.full_key == leaf_key => (*leaf).into(),
        _ => TreeEntry::empty(*leaf_key),
    }
}

impl<DB: PruneDatabase, H: HashTree> MerkleTreeRecovery<DB, H> {
    /// Reads entries with the specified keys from the tree. The entries are returned in the same order
    /// as requested. If a certain key is not present in the tree, the corresponding returned entry
    /// will be [empty](TreeEntry::is_empty()).
    #[allow(clippy::missing_panics_doc)]
    pub fn entries(&self, leaf_keys: &[Key]) -> Vec<TreeEntry> {
        load_and_transform_entries(&self.db, self.recovered_version(), leaf_keys, extract_entry)
            .unwrap_or_else(|_| {
                // If there's no recovered version, the recovered tree is empty yet.
                leaf_keys.iter().map(|key| TreeEntry::empty(*key)).collect()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatchSet;

    #[test]
    fn entries_in_empty_tree() {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        tree.extend(vec![]).unwrap();
        let missing_key = Key::from(123);

        let entries = tree.entries(0, &[missing_key]).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_empty());

        let entries = tree.entries_with_proofs(0, &[missing_key]).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].base.is_empty());
        entries[0]
            .verify(&tree.hasher, tree.hasher.empty_tree_hash())
            .unwrap();
    }

    #[test]
    fn entries_in_single_node_tree() {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        let key = Key::from(987_654);
        let output = tree
            .extend(vec![TreeEntry::new(key, 1, ValueHash::repeat_byte(1))])
            .unwrap();
        let missing_key = Key::from(123);

        let entries = tree.entries(0, &[key, missing_key]).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].value, ValueHash::repeat_byte(1));
        assert_eq!(entries[0].leaf_index, 1);

        let entries = tree.entries_with_proofs(0, &[key, missing_key]).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(!entries[0].base.is_empty());
        entries[0].verify(&tree.hasher, output.root_hash).unwrap();
        assert!(entries[1].base.is_empty());
        entries[1].verify(&tree.hasher, output.root_hash).unwrap();
    }
}

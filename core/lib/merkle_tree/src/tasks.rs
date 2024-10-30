//! Service tasks for the Merkle tree.

use std::time::Instant;

use anyhow::Context as _;

use crate::{types::NodeKey, PruneDatabase, RocksDBWrapper};

/// Output of a [`StaleKeysRepairTask`].
#[derive(Debug, Default)]
pub struct StaleKeysRepairOutput {
    /// Detected bogus stale keys.
    pub bogus_stale_keys: Vec<NodeKey>,
}

/// Task that repairs stale keys for the tree.
///
/// Early tree versions contained a bug: If a tree version was truncated, stale keys for it remained intact.
/// If an overwritten tree version did not contain the same keys, this led to keys incorrectly marked as stale,
/// meaning that after pruning, a tree may end up broken.
#[derive(Debug)]
pub struct StaleKeysRepairTask {
    db: RocksDBWrapper,
}

impl StaleKeysRepairTask {
    /// Creates a new task.
    pub fn new(db: RocksDBWrapper) -> Self {
        Self { db }
    }

    /// Runs stale key detection for a single tree version.
    #[tracing::instrument(skip(self))]
    pub fn run_for_version(
        &mut self,
        version: u64,
        dry_run: bool,
    ) -> anyhow::Result<StaleKeysRepairOutput> {
        const SAMPLE_COUNT: usize = 5;

        let version_keys = self
            .db
            .all_keys_for_version(version)
            .with_context(|| format!("failed loading keys changed in tree version {version}"))?;
        let stale_keys = self.db.stale_keys(version);

        if !version_keys.unreachable_keys.is_empty() {
            let keys_sample: Vec<_> = version_keys
                .unreachable_keys
                .iter()
                .take(SAMPLE_COUNT)
                .collect::<Vec<_>>();
            tracing::warn!(
                version,
                unreachable_keys.len = version_keys.unreachable_keys.len(),
                unreachable_keys.sample = ?keys_sample,
                "Found unreachable keys in tree"
            );
        }

        let mut bogus_stale_keys = vec![];
        for stale_key in stale_keys {
            if version_keys.valid_keys.contains(&stale_key.nibbles) {
                // Normal case: a new node obsoletes a previous version.
            } else if version_keys.unreachable_keys.contains(&stale_key.nibbles) {
                // Explainable bogus stale key: a node that was updated in `version` before the truncation is no longer updated after truncation.
                bogus_stale_keys.push(stale_key);
            } else {
                tracing::warn!(
                    version,
                    ?stale_key,
                    "Unexplained bogus stale key: not present in any nodes changed in the tree version"
                );
                bogus_stale_keys.push(stale_key);
            }
        }

        if bogus_stale_keys.is_empty() {
            return Ok(StaleKeysRepairOutput::default());
        }

        let keys_sample: Vec<_> = bogus_stale_keys.iter().take(SAMPLE_COUNT).collect();
        tracing::info!(
            stale_keys.len = bogus_stale_keys.len(),
            stale_keys.sample = ?keys_sample,
            "Found bogus stale keys"
        );
        if !dry_run {
            tracing::info!("Removing bogus stale keys from RocksDB");
            let started_at = Instant::now();
            self.db
                .remove_stale_keys(version, &bogus_stale_keys)
                .context("failed removing bogus stale keys")?;
            let latency = started_at.elapsed();
            tracing::info!(?latency, "Removing bogus stale keys from RocksDB");
        }

        Ok(StaleKeysRepairOutput { bogus_stale_keys })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, MerkleTree, MerkleTreePruner, TreeEntry, ValueHash};

    fn setup_tree_with_bogus_stale_keys(db: impl PruneDatabase) {
        let mut tree = MerkleTree::new(db).unwrap();
        let kvs: Vec<_> = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::zero()))
            .collect();
        tree.extend(kvs).unwrap();

        let overridden_kvs = vec![TreeEntry::new(
            Key::from(0),
            1,
            ValueHash::repeat_byte(0xaa),
        )];
        tree.extend(overridden_kvs).unwrap();

        let stale_keys = tree.db.stale_keys(1);
        assert!(
            stale_keys.iter().any(|key| !key.is_empty()),
            "{stale_keys:?}"
        );

        // Revert `overridden_kvs`.
        tree.truncate_recent_versions_incorrectly(1).unwrap();
        assert_eq!(tree.latest_version(), Some(0));
        let future_stale_keys = tree.db.stale_keys(1);
        assert!(!future_stale_keys.is_empty());

        // Add a new version without the key. To make the matter more egregious, the inserted key
        // differs from all existing keys, starting from the first nibble.
        let new_key = Key::from_big_endian(&[0xaa; 32]);
        let new_kvs = vec![TreeEntry::new(new_key, 101, ValueHash::repeat_byte(0xaa))];
        tree.extend(new_kvs).unwrap();
        assert_eq!(tree.latest_version(), Some(1));
    }

    #[test]
    fn stale_keys_repair_with_normal_tree() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();

        let mut task = StaleKeysRepairTask::new(db);
        // The task should work fine with future tree versions.
        for version in [0, 1, 100] {
            let output = task.run_for_version(version, true).unwrap();
            assert!(output.bogus_stale_keys.is_empty());
            let output = task.run_for_version(version, false).unwrap();
            assert!(output.bogus_stale_keys.is_empty());
        }

        let kvs: Vec<_> = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::zero()))
            .collect();
        MerkleTree::new(&mut task.db).unwrap().extend(kvs).unwrap();

        let output = task.run_for_version(0, false).unwrap();
        assert!(output.bogus_stale_keys.is_empty());
    }

    #[test]
    fn detecting_bogus_stale_keys() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        setup_tree_with_bogus_stale_keys(&mut db);

        let mut task = StaleKeysRepairTask::new(db);
        let output = task.run_for_version(1, true).unwrap();
        assert!(!output.bogus_stale_keys.is_empty());

        let output = task.run_for_version(1, false).unwrap();
        assert!(!output.bogus_stale_keys.is_empty());

        // Check that the tree works fine once it's pruned.
        let (mut pruner, _) = MerkleTreePruner::new(&mut task.db);
        pruner.prune_up_to(1).unwrap().expect("tree was not pruned");

        MerkleTree::new(&mut task.db)
            .unwrap()
            .verify_consistency(1, false)
            .unwrap();

        let output = task.run_for_version(1, false).unwrap();
        assert!(output.bogus_stale_keys.is_empty());
        MerkleTree::new(&mut task.db)
            .unwrap()
            .verify_consistency(1, false)
            .unwrap();
    }
}

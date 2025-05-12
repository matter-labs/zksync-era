//! Tree pruning logic.

use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Weak,
    },
    time::Duration,
};

use crate::{
    metrics::{PruningStats, PRUNING_TIMINGS},
    storage::{PruneDatabase, PrunePatchSet},
};

/// Error returned by [`MerkleTreePrunerHandle::set_target_retained_version()`].
#[derive(Debug)]
pub struct PrunerStoppedError(());

impl fmt::Display for PrunerStoppedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Merkle tree pruner stopped")
    }
}

/// Handle for a [`MerkleTreePruner`] allowing to abort its operation.
///
/// The pruner is aborted once the handle is dropped.
#[must_use = "Pruner is aborted once handle is dropped"]
#[derive(Debug)]
pub struct MerkleTreePrunerHandle {
    _aborted_sender: mpsc::Sender<()>,
    target_retained_version: Weak<AtomicU64>,
}

impl MerkleTreePrunerHandle {
    /// Sets the version of the tree the pruner should attempt to prune to. Calls should provide
    /// monotonically increasing versions; call with a lesser version will have no effect.
    ///
    /// Returns the previously set target retained version.
    ///
    /// # Errors
    ///
    /// If the pruner has stopped (e.g., due to a panic), this method will return an error.
    pub fn set_target_retained_version(&self, new_version: u64) -> Result<u64, PrunerStoppedError> {
        if let Some(version) = self.target_retained_version.upgrade() {
            Ok(version.fetch_max(new_version, Ordering::Relaxed))
        } else {
            Err(PrunerStoppedError(()))
        }
    }
}

/// Component responsible for Merkle tree pruning, i.e. removing nodes not referenced by new versions
/// of the tree.
///
/// A pruner should be instantiated using a [`Clone`] of the tree database, possibly
/// configured and then [`run()`](Self::run()) on its own thread. [`MerkleTreePrunerHandle`] provides
/// a way to gracefully shut down the pruner.
///
/// # Implementation details
///
/// Pruning works by recording stale node keys each time the Merkle tree is updated; in RocksDB,
/// stale keys are recorded in a separate column family. A pruner takes stale keys that were produced
/// by a certain range of tree versions, and removes the corresponding nodes from the tree
/// (in RocksDB, this uses simple pointwise `delete_cf()` operations). The range of versions
/// depends on pruning policies; for now, it's passed via the pruner handle.
pub struct MerkleTreePruner<DB> {
    db: DB,
    target_pruned_key_count: usize,
    poll_interval: Duration,
    aborted_receiver: mpsc::Receiver<()>,
    target_retained_version: Arc<AtomicU64>,
}

impl<DB> fmt::Debug for MerkleTreePruner<DB> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MerkleTreePruner")
            .field("target_pruned_key_count", &self.target_pruned_key_count)
            .field("poll_interval", &self.poll_interval)
            .field("target_retained_version", &self.target_retained_version)
            .finish_non_exhaustive()
    }
}

impl<DB: PruneDatabase> MerkleTreePruner<DB> {
    /// Creates a pruner with the specified database.
    ///
    /// # Return value
    ///
    /// Returns the created pruner and a handle to it. *The pruner will be aborted when its handle is dropped.*
    pub fn new(db: DB) -> (Self, MerkleTreePrunerHandle) {
        let (aborted_sender, aborted_receiver) = mpsc::channel();
        let target_retained_version = Arc::new(AtomicU64::new(0));
        let handle = MerkleTreePrunerHandle {
            _aborted_sender: aborted_sender,
            target_retained_version: Arc::downgrade(&target_retained_version),
        };
        let this = Self {
            db,
            target_pruned_key_count: 500_000,
            poll_interval: Duration::from_secs(60),
            aborted_receiver,
            target_retained_version,
        };
        (this, handle)
    }

    /// Sets the target number of stale keys pruned on a single iteration. This limits the size of
    /// a produced RocksDB `WriteBatch` and the RAM consumption of the pruner. At the same time,
    /// larger values can lead to more efficient RocksDB compaction.
    ///
    /// Reasonable values are order of 100k – 1M. The default value is 500k.
    pub fn set_target_pruned_key_count(&mut self, count: usize) {
        self.target_pruned_key_count = count;
    }

    /// Sets the sleep duration when the pruner cannot progress. This time should be enough
    /// for the tree to produce enough stale keys.
    ///
    /// The default value is 60 seconds.
    pub fn set_poll_interval(&mut self, poll_interval: Duration) {
        self.poll_interval = poll_interval;
    }

    /// Returns max version number that can be safely pruned, so that there is at least one version present after pruning.
    #[doc(hidden)] // Used in integration tests; logically private
    pub fn last_prunable_version(&self) -> Option<u64> {
        let manifest = self.db.manifest()?;
        manifest.version_count.checked_sub(1)
    }

    #[doc(hidden)] // Used in integration tests; logically private
    #[allow(clippy::range_plus_one)] // exclusive range is required by `PrunePatchSet` constructor
    pub fn prune_up_to(
        &mut self,
        target_retained_version: u64,
    ) -> anyhow::Result<Option<PruningStats>> {
        let Some(min_stale_key_version) = self.db.min_stale_key_version() else {
            return Ok(None);
        };

        // We must retain at least one tree version.
        let Some(last_prunable_version) = self.last_prunable_version() else {
            tracing::debug!("Nothing to prune; skipping");
            return Ok(None);
        };
        let target_retained_version = last_prunable_version.min(target_retained_version);
        let stale_key_new_versions = min_stale_key_version..=target_retained_version;
        if stale_key_new_versions.is_empty() {
            tracing::debug!(
                "No Merkle tree versions can be pruned; min stale key version is {min_stale_key_version}, \
                 target retained version is {target_retained_version}"
            );
            return Ok(None);
        }
        tracing::info!("Collecting stale keys with new versions in {stale_key_new_versions:?}");

        let load_stale_keys_latency = PRUNING_TIMINGS.load_stale_keys.start();
        let mut pruned_keys = vec![];
        let mut max_stale_key_version = min_stale_key_version;
        for version in stale_key_new_versions {
            max_stale_key_version = version;
            pruned_keys.extend_from_slice(&self.db.stale_keys(version));
            if pruned_keys.len() >= self.target_pruned_key_count {
                break;
            }
        }
        let load_stale_keys_latency = load_stale_keys_latency.observe();

        if pruned_keys.is_empty() {
            tracing::debug!("No stale keys to remove; skipping");
            return Ok(None);
        }
        let deleted_stale_key_versions = min_stale_key_version..(max_stale_key_version + 1);
        tracing::info!(
            "Collected {} stale keys with new versions in {deleted_stale_key_versions:?} in {load_stale_keys_latency:?}",
            pruned_keys.len()
        );

        let stats = PruningStats {
            target_retained_version,
            pruned_key_count: pruned_keys.len(),
            deleted_stale_key_versions: deleted_stale_key_versions.clone(),
        };
        let patch = PrunePatchSet::new(pruned_keys, deleted_stale_key_versions);
        let apply_patch_latency = PRUNING_TIMINGS.apply_patch.start();
        self.db.prune(patch)?;
        let apply_patch_latency = apply_patch_latency.observe();
        tracing::info!("Pruned stale keys in {apply_patch_latency:?}: {stats:?}");
        Ok(Some(stats))
    }

    fn wait_for_abort(&mut self, timeout: Duration) -> bool {
        match self.aborted_receiver.recv_timeout(timeout) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The pruner handle is alive and wasn't used to abort the pruner.
                false
            }
        }
    }

    /// Runs this pruner indefinitely until it is aborted, or a database error occurs.
    ///
    /// # Errors
    ///
    /// Propagates database I/O errors.
    pub fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Started Merkle tree pruner {self:?}");

        let mut wait_interval = Duration::ZERO;
        while !self.wait_for_abort(wait_interval) {
            let retained_version = self.target_retained_version.load(Ordering::Relaxed);
            wait_interval = if let Some(stats) = self.prune_up_to(retained_version)? {
                tracing::debug!(
                    "Performed pruning for target retained version {retained_version}: {stats:?}"
                );
                stats.report();
                if stats.has_more_work() {
                    // Continue pruning right away instead of waiting for abort.
                    Duration::ZERO
                } else {
                    self.poll_interval
                }
            } else {
                tracing::debug!(
                    "Pruning was not performed; waiting {:?}",
                    self.poll_interval
                );
                self.poll_interval
            };
        }
        tracing::info!("Stop request received, tree pruning is shut down");
        Ok(())
    }
}

impl PruningStats {
    fn has_more_work(&self) -> bool {
        self.target_retained_version + 1 > self.deleted_stale_key_versions.end
    }
}

#[allow(clippy::range_plus_one)] // required for comparisons
#[cfg(test)]
mod tests {
    use std::{collections::HashSet, thread, time::Instant};

    use super::*;
    use crate::{
        types::{Node, NodeKey},
        utils::testonly::setup_tree_with_stale_keys,
        Database, Key, MerkleTree, PatchSet, RocksDBWrapper, TreeEntry, ValueHash,
    };

    fn create_db() -> PatchSet {
        let mut db = PatchSet::default();
        for i in 0..5 {
            let key = Key::from(i);
            let value = ValueHash::from_low_u64_be(i);
            MerkleTree::new(&mut db)
                .unwrap()
                .extend(vec![TreeEntry::new(key, i + 1, value)])
                .unwrap();
        }
        db
    }

    #[test]
    fn pruner_basics() {
        let mut db = create_db();
        assert_eq!(
            MerkleTree::new(&mut db).unwrap().first_retained_version(),
            Some(0)
        );

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
        let stats = pruner
            .prune_up_to(pruner.last_prunable_version().unwrap())
            .unwrap()
            .expect("tree was not pruned");
        assert!(stats.pruned_key_count > 0);
        assert_eq!(stats.deleted_stale_key_versions, 1..5);
        assert_eq!(stats.target_retained_version, 4);
        assert!(!stats.has_more_work());

        // Check the `PatchSet` implementation of `PruneDatabase`.
        for version in 0..4 {
            assert!(db.root_mut(version).is_none());
        }
        assert!(db.root_mut(4).is_some());

        assert_eq!(
            MerkleTree::new(&mut db).unwrap().first_retained_version(),
            Some(4)
        );
    }

    #[test]
    fn pruner_with_intermediate_commits() {
        let mut db = create_db();
        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
        pruner.set_target_pruned_key_count(1);

        for i in 1..5 {
            let stats = pruner
                .prune_up_to(pruner.last_prunable_version().unwrap())
                .unwrap()
                .expect("tree was not pruned");
            assert!(stats.pruned_key_count > 0);
            assert_eq!(stats.deleted_stale_key_versions, i..(i + 1));
            assert_eq!(stats.target_retained_version, 4);
            assert_eq!(stats.has_more_work(), i != 4);
        }
    }

    #[test]
    fn pruner_is_aborted_immediately_when_requested() {
        let (mut pruner, pruner_handle) = MerkleTreePruner::new(PatchSet::default());
        pruner.set_poll_interval(Duration::from_secs(30));
        let join_handle = thread::spawn(|| pruner.run());

        drop(pruner_handle);
        let start = Instant::now();
        join_handle.join().unwrap().unwrap();
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    fn generate_key_value_pairs(indexes: impl Iterator<Item = u64>) -> Vec<TreeEntry> {
        indexes
            .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::from_low_u64_be(i)))
            .collect()
    }

    fn test_tree_is_consistent_after_pruning(past_versions_to_keep: u64) {
        let mut db = PatchSet::default();
        let mut tree = MerkleTree::new(&mut db).unwrap();
        let kvs = generate_key_value_pairs(0..100);
        for chunk in kvs.chunks(20) {
            tree.extend(chunk.to_vec()).unwrap();
        }
        let latest_version = tree.latest_version().unwrap();

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
        let stats = pruner
            .prune_up_to(pruner.last_prunable_version().unwrap() - past_versions_to_keep)
            .unwrap()
            .expect("tree was not pruned");
        assert!(stats.pruned_key_count > 0);
        let first_retained_version = latest_version.saturating_sub(past_versions_to_keep);
        assert_eq!(stats.target_retained_version, first_retained_version);
        assert_eq!(
            stats.deleted_stale_key_versions,
            1..(first_retained_version + 1)
        );
        assert_no_stale_keys(&db, first_retained_version);

        let mut tree = MerkleTree::new(&mut db).unwrap();
        assert_eq!(tree.first_retained_version(), Some(first_retained_version));
        for version in first_retained_version..=latest_version {
            tree.verify_consistency(version, true).unwrap();
        }

        let kvs = generate_key_value_pairs(100..200);
        for chunk in kvs.chunks(10) {
            tree.extend(chunk.to_vec()).unwrap();
        }
        let latest_version = tree.latest_version().unwrap();

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
        let stats = pruner
            .prune_up_to(pruner.last_prunable_version().unwrap() - past_versions_to_keep)
            .unwrap()
            .expect("tree was not pruned");
        assert!(stats.pruned_key_count > 0);
        let first_retained_version = latest_version.saturating_sub(past_versions_to_keep);
        assert_eq!(stats.target_retained_version, first_retained_version);

        let tree = MerkleTree::new(&mut db).unwrap();
        for version in first_retained_version..=latest_version {
            tree.verify_consistency(version, true).unwrap();
        }
        assert_no_stale_keys(&db, first_retained_version);
    }

    fn assert_no_stale_keys(db: &PatchSet, first_retained_version: u64) {
        if let Some(version) = db.min_stale_key_version() {
            assert!(version > first_retained_version);
        }
        for version in 0..first_retained_version {
            assert!(db.root(version).is_none());
        }
    }

    #[test]
    fn tree_is_consistent_after_pruning() {
        test_tree_is_consistent_after_pruning(0);
    }

    #[test]
    fn tree_is_consistent_after_partial_pruning() {
        test_tree_is_consistent_after_pruning(2);
    }

    fn test_keys_are_removed_by_pruning_when_overwritten(initialize_iteratively: bool) {
        const ITERATIVE_BATCH_COUNT: usize = 10;

        let mut db = PatchSet::default();
        let kvs: Vec<_> = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::zero()))
            .collect();

        let batch_count = if initialize_iteratively {
            for chunk in kvs.chunks(ITERATIVE_BATCH_COUNT) {
                MerkleTree::new(&mut db)
                    .unwrap()
                    .extend(chunk.to_vec())
                    .unwrap();
            }
            ITERATIVE_BATCH_COUNT
        } else {
            MerkleTree::new(&mut db).unwrap().extend(kvs).unwrap();
            1
        };
        let keys_in_db: HashSet<_> = db.nodes_mut().map(|(key, _)| *key).collect();

        // Completely overwrite all keys.
        let new_value_hash = ValueHash::from_low_u64_be(1_000);
        let new_kvs = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, new_value_hash))
            .collect();
        MerkleTree::new(&mut db).unwrap().extend(new_kvs).unwrap();

        // Sanity check: before pruning, all old keys should be present.
        let new_keys_in_db: HashSet<_> = db.nodes_mut().map(|(key, _)| *key).collect();
        assert!(new_keys_in_db.is_superset(&keys_in_db));

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
        let stats = pruner
            .prune_up_to(pruner.last_prunable_version().unwrap())
            .unwrap()
            .expect("tree was not pruned");
        assert_eq!(stats.pruned_key_count, keys_in_db.len() + batch_count);
        // ^ roots are not counted in `keys_in_db`

        let new_keys_in_db: HashSet<_> = db.nodes_mut().map(|(key, _)| *key).collect();
        assert!(new_keys_in_db.is_disjoint(&keys_in_db));
    }

    #[test]
    fn keys_are_removed_by_pruning_when_overwritten() {
        println!("Keys are inserted in single batch");
        test_keys_are_removed_by_pruning_when_overwritten(false);
        println!("Keys are inserted in several batches");
        test_keys_are_removed_by_pruning_when_overwritten(true);
    }

    fn test_keys_are_removed_by_pruning_when_overwritten_in_multiple_batches(
        prune_iteratively: bool,
    ) {
        let mut db = PatchSet::default();
        let kvs: Vec<_> = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, ValueHash::zero()))
            .collect();
        MerkleTree::new(&mut db).unwrap().extend(kvs).unwrap();
        let leaf_keys_in_db = leaf_keys(&mut db);

        // Completely overwrite all keys in several batches.
        let new_value_hash = ValueHash::from_low_u64_be(1_000);
        let new_kvs: Vec<_> = (0_u64..100)
            .map(|i| TreeEntry::new(Key::from(i), i + 1, new_value_hash))
            .collect();
        for chunk in new_kvs.chunks(20) {
            MerkleTree::new(&mut db)
                .unwrap()
                .extend(chunk.to_vec())
                .unwrap();
            if prune_iteratively {
                let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
                pruner
                    .prune_up_to(pruner.last_prunable_version().unwrap())
                    .unwrap();
            }
        }

        if !prune_iteratively {
            let (mut pruner, _handle) = MerkleTreePruner::new(&mut db);
            pruner
                .prune_up_to(pruner.last_prunable_version().unwrap())
                .unwrap();
        }
        let new_leaf_keys_in_db = leaf_keys(&mut db);
        assert!(new_leaf_keys_in_db.is_disjoint(&leaf_keys_in_db));
    }

    fn leaf_keys(db: &mut PatchSet) -> HashSet<NodeKey> {
        db.nodes_mut()
            .filter_map(|(key, node)| matches!(node, Node::Leaf(_)).then_some(*key))
            .collect()
    }

    #[test]
    fn keys_are_removed_by_pruning_when_overwritten_in_multiple_batches() {
        println!("Keys are pruned single time");
        test_keys_are_removed_by_pruning_when_overwritten_in_multiple_batches(false);
        println!("Keys are pruned after each update");
        test_keys_are_removed_by_pruning_when_overwritten_in_multiple_batches(true);
    }

    fn test_pruning_with_truncation(mut db: impl PruneDatabase) {
        setup_tree_with_stale_keys(&mut db, false);

        let stale_keys = db.stale_keys(1);
        assert_eq!(stale_keys.len(), 1);
        assert!(
            stale_keys[0].is_empty() && stale_keys[0].version == 0,
            "{stale_keys:?}"
        );

        let (mut pruner, _) = MerkleTreePruner::new(db);
        let prunable_version = pruner.last_prunable_version().unwrap();
        assert_eq!(prunable_version, 1);
        let stats = pruner
            .prune_up_to(prunable_version)
            .unwrap()
            .expect("tree was not pruned");
        assert_eq!(stats.target_retained_version, 1);
        assert_eq!(stats.pruned_key_count, 1); // only the root node should have been pruned

        let tree = MerkleTree::new(pruner.db).unwrap();
        tree.verify_consistency(1, false).unwrap();
    }

    #[test]
    fn pruning_with_truncation() {
        test_pruning_with_truncation(PatchSet::default());
    }

    #[test]
    fn pruning_with_truncation_on_rocksdb() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        test_pruning_with_truncation(RocksDBWrapper::new(temp_dir.path()).unwrap());
    }
}

//! Tree pruning logic.

use std::{fmt, sync::mpsc, time::Duration};

use crate::{
    metrics::{PruningStats, PRUNING_TIMINGS},
    storage::{PruneDatabase, PrunePatchSet},
};

/// Handle for a [`MerkleTreePruner`] allowing to abort its operation.
///
/// The pruner is aborted once the handle is dropped.
#[must_use = "Pruner is aborted once handle is dropped"]
#[derive(Debug)]
pub struct MerkleTreePrunerHandle {
    aborted_sender: mpsc::Sender<()>,
}

impl MerkleTreePrunerHandle {
    /// Aborts the pruner that this handle is attached to. If the pruner has already terminated
    /// (e.g., due to a panic), this is a no-op.
    pub fn abort(self) {
        self.aborted_sender.send(()).ok();
    }
}

/// Component responsible for Merkle tree pruning, i.e. removing nodes not referenced by new versions
/// of the tree. A pruner should be instantiated using a [`Clone`] of the tree database, possibly
/// configured and then [`run()`](Self::run()) on its own thread. [`MerkleTreePrunerHandle`] provides
/// a way to gracefully shut down the pruner.
///
/// # Implementation details
///
/// Pruning works by recording stale node keys each time the Merkle tree is updated; in RocksDB,
/// stale keys are recorded in a separate column family. A pruner takes stale keys that were produced
/// by a certain range of tree versions, and removes the corresponding nodes from the tree
/// (in RocksDB, this uses simple pointwise `delete_cf()` operations). The range of versions
/// depends on pruning policies; for now, it's "remove versions older than `latest_version - N`",
/// where `N` is a configurable number set when the pruner [is created](Self::new()).
pub struct MerkleTreePruner<DB> {
    db: DB,
    past_versions_to_keep: u64,
    target_pruned_key_count: usize,
    poll_interval: Duration,
    aborted_receiver: mpsc::Receiver<()>,
}

impl<DB> fmt::Debug for MerkleTreePruner<DB> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MerkleTreePruner")
            .field("past_versions_to_keep", &self.past_versions_to_keep)
            .field("target_pruned_key_count", &self.target_pruned_key_count)
            .field("poll_interval", &self.poll_interval)
            .finish_non_exhaustive()
    }
}

impl<DB: PruneDatabase> MerkleTreePruner<DB> {
    /// Creates a pruner with the specified database and the number of past tree versions to keep.
    /// E.g., 0 means keeping only the latest version.
    ///
    /// # Return value
    ///
    /// Returns the created pruner and a handle to it. *The pruner will be aborted when its handle
    /// is dropped.*
    pub fn new(db: DB, past_versions_to_keep: u64) -> (Self, MerkleTreePrunerHandle) {
        let (aborted_sender, aborted_receiver) = mpsc::channel();
        let handle = MerkleTreePrunerHandle { aborted_sender };
        let this = Self {
            db,
            past_versions_to_keep,
            target_pruned_key_count: 500_000,
            poll_interval: Duration::from_secs(60),
            aborted_receiver,
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
    /// The default value is 60s.
    pub fn set_poll_interval(&mut self, poll_interval: Duration) {
        self.poll_interval = poll_interval;
    }

    fn target_retained_version(&self) -> Option<u64> {
        let manifest = self.db.manifest()?;
        let latest_version = manifest.version_count.checked_sub(1)?;
        latest_version.checked_sub(self.past_versions_to_keep)
    }

    #[doc(hidden)] // Used in integration tests; logically private
    #[allow(clippy::range_plus_one)] // exclusive range is required by `PrunePatchSet` constructor
    pub fn run_once(&mut self) -> Option<PruningStats> {
        let target_retained_version = self.target_retained_version()?;
        let min_stale_key_version = self.db.min_stale_key_version()?;
        let stale_key_new_versions = min_stale_key_version..=target_retained_version;
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
        load_stale_keys_latency.observe();

        if pruned_keys.is_empty() {
            tracing::info!("No stale keys to remove; skipping");
            return None;
        }
        let deleted_stale_key_versions = min_stale_key_version..(max_stale_key_version + 1);
        tracing::info!(
            "Collected {} stale keys with new versions in {deleted_stale_key_versions:?}",
            pruned_keys.len()
        );

        let stats = PruningStats {
            target_retained_version,
            pruned_key_count: pruned_keys.len(),
            deleted_stale_key_versions: deleted_stale_key_versions.clone(),
        };
        let patch = PrunePatchSet::new(pruned_keys, deleted_stale_key_versions);
        let apply_patch_latency = PRUNING_TIMINGS.apply_patch.start();
        self.db.prune(patch);
        apply_patch_latency.observe();
        Some(stats)
    }

    /// Runs this pruner indefinitely until it is aborted by dropping its handle.
    pub fn run(mut self) {
        tracing::info!("Started Merkle tree pruner {self:?}");
        loop {
            let timeout = if let Some(stats) = self.run_once() {
                let has_more_work = stats.has_more_work();
                stats.report();
                if has_more_work {
                    Duration::ZERO
                } else {
                    self.poll_interval
                }
            } else {
                tracing::debug!("No pruning required per specified policies; waiting");
                self.poll_interval
            };

            match self.aborted_receiver.recv_timeout(timeout) {
                Ok(()) => break, // Abort was requested
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("Pruner handle is dropped without calling `abort()`; exiting");
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // The pruner handle is alive and wasn't used to abort the pruner.
                }
            }
        }
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
        Database, Key, MerkleTree, PatchSet, ValueHash,
    };

    fn create_db() -> PatchSet {
        let mut db = PatchSet::default();
        for i in 0..5 {
            let key = Key::from(i);
            let value = ValueHash::from_low_u64_be(i);
            MerkleTree::new(&mut db).extend(vec![(key, value)]);
        }
        db
    }

    #[test]
    fn pruner_basics() {
        let mut db = create_db();
        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db, 0);

        let stats = pruner.run_once().unwrap();
        assert!(stats.pruned_key_count > 0);
        assert_eq!(stats.deleted_stale_key_versions, 1..5);
        assert_eq!(stats.target_retained_version, 4);
        assert!(!stats.has_more_work());

        // Check the `PatchSet` implementation of `PruneDatabase`.
        for version in 0..4 {
            assert!(db.root_mut(version).is_none());
        }
        assert!(db.root_mut(4).is_some());
    }

    #[test]
    fn pruner_with_intermediate_commits() {
        let mut db = create_db();
        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db, 0);
        pruner.set_target_pruned_key_count(1);

        for i in 1..5 {
            let stats = pruner.run_once().unwrap();
            assert!(stats.pruned_key_count > 0);
            assert_eq!(stats.deleted_stale_key_versions, i..(i + 1));
            assert_eq!(stats.target_retained_version, 4);
            assert_eq!(stats.has_more_work(), i != 4);
        }
    }

    #[test]
    fn pruner_is_aborted_immediately_when_requested() {
        let (mut pruner, pruner_handle) = MerkleTreePruner::new(PatchSet::default(), 0);
        pruner.set_poll_interval(Duration::from_secs(30));
        let join_handle = thread::spawn(|| pruner.run());

        pruner_handle.abort();
        let start = Instant::now();
        join_handle.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    fn generate_key_value_pairs(indexes: impl Iterator<Item = u64>) -> Vec<(Key, ValueHash)> {
        indexes
            .map(|i| (Key::from(i), ValueHash::from_low_u64_be(i)))
            .collect()
    }

    fn test_tree_is_consistent_after_pruning(past_versions_to_keep: u64) {
        let mut db = PatchSet::default();
        let mut tree = MerkleTree::new(&mut db);
        let kvs = generate_key_value_pairs(0..100);
        for chunk in kvs.chunks(20) {
            tree.extend(chunk.to_vec());
        }
        let latest_version = tree.latest_version().unwrap();

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db, past_versions_to_keep);
        let stats = pruner.run_once().unwrap();
        assert!(stats.pruned_key_count > 0);
        let first_retained_version = latest_version.saturating_sub(past_versions_to_keep);
        assert_eq!(stats.target_retained_version, first_retained_version);
        assert_eq!(
            stats.deleted_stale_key_versions,
            1..(first_retained_version + 1)
        );
        assert_no_stale_keys(&db, first_retained_version);

        let mut tree = MerkleTree::new(&mut db);
        for version in first_retained_version..=latest_version {
            tree.verify_consistency(version).unwrap();
        }

        let kvs = generate_key_value_pairs(100..200);
        for chunk in kvs.chunks(10) {
            tree.extend(chunk.to_vec());
        }
        let latest_version = tree.latest_version().unwrap();

        let (mut pruner, _handle) = MerkleTreePruner::new(&mut db, past_versions_to_keep);
        let stats = pruner.run_once().unwrap();
        assert!(stats.pruned_key_count > 0);
        let first_retained_version = latest_version.saturating_sub(past_versions_to_keep);
        assert_eq!(stats.target_retained_version, first_retained_version);

        let tree = MerkleTree::new(&mut db);
        for version in first_retained_version..=latest_version {
            tree.verify_consistency(version).unwrap();
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
        let kvs: Vec<_> = (0_u32..100)
            .map(|i| (Key::from(i), ValueHash::zero()))
            .collect();

        let batch_count = if initialize_iteratively {
            for chunk in kvs.chunks(ITERATIVE_BATCH_COUNT) {
                MerkleTree::new(&mut db).extend(chunk.to_vec());
            }
            ITERATIVE_BATCH_COUNT
        } else {
            MerkleTree::new(&mut db).extend(kvs);
            1
        };
        let keys_in_db: HashSet<_> = db.nodes_mut().map(|(key, _)| *key).collect();

        // Completely overwrite all keys.
        let new_value_hash = ValueHash::from_low_u64_be(1_000);
        let new_kvs = (0_u32..100)
            .map(|i| (Key::from(i), new_value_hash))
            .collect();
        MerkleTree::new(&mut db).extend(new_kvs);

        // Sanity check: before pruning, all old keys should be present.
        let new_keys_in_db: HashSet<_> = db.nodes_mut().map(|(key, _)| *key).collect();
        assert!(new_keys_in_db.is_superset(&keys_in_db));

        let stats = MerkleTreePruner::new(&mut db, 0).0.run_once().unwrap();
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
        let kvs: Vec<_> = (0_u32..100)
            .map(|i| (Key::from(i), ValueHash::zero()))
            .collect();
        MerkleTree::new(&mut db).extend(kvs);
        let leaf_keys_in_db = leaf_keys(&mut db);

        // Completely overwrite all keys in several batches.
        let new_value_hash = ValueHash::from_low_u64_be(1_000);
        let new_kvs: Vec<_> = (0_u32..100)
            .map(|i| (Key::from(i), new_value_hash))
            .collect();
        for chunk in new_kvs.chunks(20) {
            MerkleTree::new(&mut db).extend(chunk.to_vec());
            if prune_iteratively {
                MerkleTreePruner::new(&mut db, 0).0.run_once().unwrap();
            }
        }

        if !prune_iteratively {
            MerkleTreePruner::new(&mut db, 0).0.run_once().unwrap();
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
}

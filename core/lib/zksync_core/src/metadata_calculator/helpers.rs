//! Various helpers for the metadata calculator.

use std::{
    collections::BTreeMap,
    future,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_config::configs::database::MerkleTreeMode;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus};
use zksync_merkle_tree::{
    domain::{TreeMetadata, ZkSyncTree, ZkSyncTreeReader},
    recovery::MerkleTreeRecovery,
    Database, Key, NoVersionError, RocksDBWrapper, TreeEntry, TreeEntryWithProof, TreeInstruction,
};
use zksync_storage::{RocksDB, RocksDBOptions, StalledWritesRetries};
use zksync_types::{block::L1BatchHeader, L1BatchNumber, StorageKey, H256};

use super::metrics::{LoadChangesStage, TreeUpdateStage, METRICS};

/// General information about the Merkle tree.
#[derive(Debug, Serialize, Deserialize)]
pub struct MerkleTreeInfo {
    pub mode: MerkleTreeMode,
    pub root_hash: H256,
    pub next_l1_batch_number: L1BatchNumber,
    pub leaf_count: u64,
}

/// Health details for a Merkle tree.
#[derive(Debug, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub(super) enum MerkleTreeHealth {
    Initialization,
    Recovery {
        chunk_count: u64,
        recovered_chunk_count: u64,
    },
    MainLoop(MerkleTreeInfo),
}

impl From<MerkleTreeHealth> for Health {
    fn from(details: MerkleTreeHealth) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

impl From<MerkleTreeInfo> for Health {
    fn from(info: MerkleTreeInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(MerkleTreeHealth::MainLoop(info))
    }
}

/// Creates a RocksDB wrapper with the specified params.
pub(super) async fn create_db(
    path: PathBuf,
    block_cache_capacity: usize,
    memtable_capacity: usize,
    stalled_writes_timeout: Duration,
    multi_get_chunk_size: usize,
) -> anyhow::Result<RocksDBWrapper> {
    tokio::task::spawn_blocking(move || {
        create_db_sync(
            &path,
            block_cache_capacity,
            memtable_capacity,
            stalled_writes_timeout,
            multi_get_chunk_size,
        )
    })
    .await
    .context("panicked creating Merkle tree RocksDB")?
}

fn create_db_sync(
    path: &Path,
    block_cache_capacity: usize,
    memtable_capacity: usize,
    stalled_writes_timeout: Duration,
    multi_get_chunk_size: usize,
) -> anyhow::Result<RocksDBWrapper> {
    tracing::info!(
        "Initializing Merkle tree database at `{path}` with {multi_get_chunk_size} multi-get chunk size, \
         {block_cache_capacity}B block cache, {memtable_capacity}B memtable capacity, \
         {stalled_writes_timeout:?} stalled writes timeout",
        path = path.display()
    );

    let mut db = RocksDB::with_options(
        path,
        RocksDBOptions {
            block_cache_capacity: Some(block_cache_capacity),
            large_memtable_capacity: Some(memtable_capacity),
            stalled_writes_retries: StalledWritesRetries::new(stalled_writes_timeout),
            max_open_files: None,
        },
    )?;
    if cfg!(test) {
        // We need sync writes for the unit tests to execute reliably. With the default config,
        // some writes to RocksDB may occur, but not be visible to the test code.
        db = db.with_sync_writes();
    }
    let mut db = RocksDBWrapper::from(db);
    db.set_multi_get_chunk_size(multi_get_chunk_size);
    Ok(db)
}

/// Wrapper around the "main" tree implementation used by [`MetadataCalculator`].
///
/// Async methods provided by this wrapper are not cancel-safe! This is probably not an issue;
/// `ZkSyncTree` is only indirectly available via `MetadataCalculator::run()` entrypoint
/// which consumes `self`. That is, if `MetadataCalculator::run()` is canceled (which we don't currently do,
/// at least not explicitly), all `MetadataCalculator` data including `ZkSyncTree` is discarded.
/// In the unlikely case you get a "`ZkSyncTree` is in inconsistent state" panic,
/// cancellation is most probably the reason.
#[derive(Debug)]
pub(super) struct AsyncTree {
    inner: Option<ZkSyncTree>,
    mode: MerkleTreeMode,
}

impl AsyncTree {
    const INCONSISTENT_MSG: &'static str =
        "`AsyncTree` is in inconsistent state, which could occur after one of its async methods was cancelled";

    pub fn new(db: RocksDBWrapper, mode: MerkleTreeMode) -> Self {
        let tree = match mode {
            MerkleTreeMode::Full => ZkSyncTree::new(db),
            MerkleTreeMode::Lightweight => ZkSyncTree::new_lightweight(db),
        };
        Self {
            inner: Some(tree),
            mode,
        }
    }

    fn as_ref(&self) -> &ZkSyncTree {
        self.inner.as_ref().expect(Self::INCONSISTENT_MSG)
    }

    fn as_mut(&mut self) -> &mut ZkSyncTree {
        self.inner.as_mut().expect(Self::INCONSISTENT_MSG)
    }

    pub fn reader(&self) -> AsyncTreeReader {
        AsyncTreeReader {
            inner: self.inner.as_ref().expect(Self::INCONSISTENT_MSG).reader(),
            mode: self.mode,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    pub fn next_l1_batch_number(&self) -> L1BatchNumber {
        self.as_ref().next_l1_batch_number()
    }

    #[cfg(test)]
    pub fn root_hash(&self) -> H256 {
        self.as_ref().root_hash()
    }

    pub async fn process_l1_batch(
        &mut self,
        storage_logs: Vec<TreeInstruction<StorageKey>>,
    ) -> TreeMetadata {
        let mut tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let (tree, metadata) = tokio::task::spawn_blocking(move || {
            let metadata = tree.process_l1_batch(&storage_logs);
            (tree, metadata)
        })
        .await
        .unwrap();

        self.inner = Some(tree);
        metadata
    }

    pub async fn save(&mut self) {
        let mut tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        self.inner = Some(
            tokio::task::spawn_blocking(|| {
                tree.save();
                tree
            })
            .await
            .unwrap(),
        );
    }

    pub fn revert_logs(&mut self, last_l1_batch_to_keep: L1BatchNumber) {
        self.as_mut().revert_logs(last_l1_batch_to_keep);
    }
}

/// Async version of [`ZkSyncTreeReader`].
#[derive(Debug, Clone)]
pub(crate) struct AsyncTreeReader {
    inner: ZkSyncTreeReader,
    mode: MerkleTreeMode,
}

impl AsyncTreeReader {
    pub async fn info(self) -> MerkleTreeInfo {
        tokio::task::spawn_blocking(move || MerkleTreeInfo {
            mode: self.mode,
            root_hash: self.inner.root_hash(),
            next_l1_batch_number: self.inner.next_l1_batch_number(),
            leaf_count: self.inner.leaf_count(),
        })
        .await
        .unwrap()
    }

    pub async fn entries_with_proofs(
        self,
        l1_batch_number: L1BatchNumber,
        keys: Vec<Key>,
    ) -> Result<Vec<TreeEntryWithProof>, NoVersionError> {
        tokio::task::spawn_blocking(move || self.inner.entries_with_proofs(l1_batch_number, &keys))
            .await
            .unwrap()
    }
}

/// Lazily initialized [`AsyncTreeReader`].
#[derive(Debug)]
pub struct LazyAsyncTreeReader(pub(super) watch::Receiver<Option<AsyncTreeReader>>);

impl LazyAsyncTreeReader {
    /// Returns a reader if it is initialized.
    pub(crate) fn read(&self) -> Option<AsyncTreeReader> {
        self.0.borrow().clone()
    }

    /// Waits until the tree is initialized and returns a reader for it.
    pub(crate) async fn wait(mut self) -> AsyncTreeReader {
        loop {
            if let Some(reader) = self.0.borrow().clone() {
                break reader;
            }
            if self.0.changed().await.is_err() {
                tracing::info!("Tree dropped without getting ready; not resolving tree reader");
                future::pending::<()>().await;
            }
        }
    }
}

/// Async wrapper for [`MerkleTreeRecovery`].
#[derive(Debug, Default)]
pub(super) struct AsyncTreeRecovery {
    inner: Option<MerkleTreeRecovery<RocksDBWrapper>>,
    mode: MerkleTreeMode,
}

impl AsyncTreeRecovery {
    const INCONSISTENT_MSG: &'static str =
        "`AsyncTreeRecovery` is in inconsistent state, which could occur after one of its async methods was cancelled";

    pub fn new(db: RocksDBWrapper, recovered_version: u64, mode: MerkleTreeMode) -> Self {
        Self {
            inner: Some(MerkleTreeRecovery::new(db, recovered_version)),
            mode,
        }
    }

    pub fn recovered_version(&self) -> u64 {
        self.inner
            .as_ref()
            .expect(Self::INCONSISTENT_MSG)
            .recovered_version()
    }

    /// Returns an entry for the specified key.
    pub async fn entries(&mut self, keys: Vec<Key>) -> Vec<TreeEntry> {
        let tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let (entry, tree) = tokio::task::spawn_blocking(move || (tree.entries(&keys), tree))
            .await
            .unwrap();
        self.inner = Some(tree);
        entry
    }

    /// Returns the current hash of the tree.
    pub async fn root_hash(&mut self) -> H256 {
        let tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let (root_hash, tree) = tokio::task::spawn_blocking(move || (tree.root_hash(), tree))
            .await
            .unwrap();
        self.inner = Some(tree);
        root_hash
    }

    /// Extends the tree with a chunk of recovery entries.
    pub async fn extend(&mut self, entries: Vec<TreeEntry>) {
        let mut tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let tree = tokio::task::spawn_blocking(move || {
            tree.extend_random(entries);
            tree
        })
        .await
        .unwrap();

        self.inner = Some(tree);
    }

    pub async fn finalize(self) -> AsyncTree {
        let tree = self.inner.expect(Self::INCONSISTENT_MSG);
        let db = tokio::task::spawn_blocking(|| tree.finalize())
            .await
            .unwrap();
        AsyncTree::new(db, self.mode)
    }
}

/// Tree at any stage of its life cycle.
#[derive(Debug)]
pub(super) enum GenericAsyncTree {
    /// Uninitialized tree.
    Empty {
        db: RocksDBWrapper,
        mode: MerkleTreeMode,
    },
    /// The tree during recovery.
    Recovering(AsyncTreeRecovery),
    /// Tree that is fully recovered and can operate normally.
    Ready(AsyncTree),
}

impl GenericAsyncTree {
    pub async fn new(db: RocksDBWrapper, mode: MerkleTreeMode) -> Self {
        tokio::task::spawn_blocking(move || {
            let Some(manifest) = db.manifest() else {
                return Self::Empty { db, mode };
            };
            if let Some(version) = manifest.recovered_version() {
                Self::Recovering(AsyncTreeRecovery::new(db, version, mode))
            } else {
                Self::Ready(AsyncTree::new(db, mode))
            }
        })
        .await
        .unwrap()
    }
}

/// Component implementing the delay policy in [`MetadataCalculator`] when there are no
/// L1 batches to seal.
#[derive(Debug, Clone)]
pub(super) struct Delayer {
    delay_interval: Duration,
    // Notifies the tests about the next L1 batch number and tree root hash when the calculator
    // runs out of L1 batches to process. (Since RocksDB is exclusive, we cannot just create
    // another instance to check these params on the test side without stopping the calculation.)
    #[cfg(test)]
    pub delay_notifier: mpsc::UnboundedSender<(L1BatchNumber, H256)>,
}

impl Delayer {
    pub fn new(delay_interval: Duration) -> Self {
        Self {
            delay_interval,
            #[cfg(test)]
            delay_notifier: mpsc::unbounded_channel().0,
        }
    }

    pub fn delay_interval(&self) -> Duration {
        self.delay_interval
    }

    #[cfg_attr(not(test), allow(unused))] // `tree` is only used in test mode
    pub fn wait(&self, tree: &AsyncTree) -> impl Future<Output = ()> {
        #[cfg(test)]
        self.delay_notifier
            .send((tree.next_l1_batch_number(), tree.root_hash()))
            .ok();
        tokio::time::sleep(self.delay_interval)
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct L1BatchWithLogs {
    pub header: L1BatchHeader,
    pub storage_logs: Vec<TreeInstruction<StorageKey>>,
}

impl L1BatchWithLogs {
    pub async fn new(
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> Option<Self> {
        tracing::debug!("Loading storage logs data for L1 batch #{l1_batch_number}");
        let load_changes_latency = METRICS.start_stage(TreeUpdateStage::LoadChanges);

        let header_latency = METRICS.start_load_stage(LoadChangesStage::LoadL1BatchHeader);
        let header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()?;
        header_latency.observe();

        let protective_reads_latency =
            METRICS.start_load_stage(LoadChangesStage::LoadProtectiveReads);
        let protective_reads = storage
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(l1_batch_number)
            .await;
        protective_reads_latency.observe_with_count(protective_reads.len());

        let touched_slots_latency = METRICS.start_load_stage(LoadChangesStage::LoadTouchedSlots);
        let mut touched_slots = storage
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(l1_batch_number)
            .await
            .unwrap();
        touched_slots_latency.observe_with_count(touched_slots.len());

        let leaf_indices_latency = METRICS.start_load_stage(LoadChangesStage::LoadLeafIndices);
        let hashed_keys_for_writes: Vec<_> =
            touched_slots.keys().map(StorageKey::hashed_key).collect();
        let l1_batches_for_initial_writes = storage
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&hashed_keys_for_writes)
            .await
            .unwrap();
        leaf_indices_latency.observe_with_count(hashed_keys_for_writes.len());

        let mut storage_logs = BTreeMap::new();
        for storage_key in protective_reads {
            touched_slots.remove(&storage_key);
            // ^ As per deduplication rules, all keys in `protective_reads` haven't *really* changed
            // in the considered L1 batch. Thus, we can remove them from `touched_slots` in order to simplify
            // their further processing.
            let log = TreeInstruction::Read(storage_key);
            storage_logs.insert(storage_key, log);
        }
        tracing::debug!(
            "Made touched slots disjoint with protective reads; remaining touched slots: {}",
            touched_slots.len()
        );

        for (storage_key, value) in touched_slots {
            if let Some(&(initial_write_batch_for_key, leaf_index)) =
                l1_batches_for_initial_writes.get(&storage_key.hashed_key())
            {
                // Filter out logs that correspond to deduplicated writes.
                if initial_write_batch_for_key <= l1_batch_number {
                    storage_logs.insert(
                        storage_key,
                        TreeInstruction::write(storage_key, leaf_index, value),
                    );
                }
            }
        }

        load_changes_latency.observe();
        Some(Self {
            header,
            storage_logs: storage_logs.into_values().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_prover_interface::inputs::PrepareBasicCircuitsJob;
    use zksync_types::{StorageKey, StorageLog};

    use super::*;
    use crate::{
        genesis::{insert_genesis_batch, GenesisParams},
        metadata_calculator::tests::{extend_db_state, gen_storage_logs, reset_db_state},
    };

    impl L1BatchWithLogs {
        /// Old, slower method of loading storage logs. We want to test its equivalence to the new implementation.
        async fn slow(
            storage: &mut Connection<'_, Core>,
            l1_batch_number: L1BatchNumber,
        ) -> Option<Self> {
            let header = storage
                .blocks_dal()
                .get_l1_batch_header(l1_batch_number)
                .await
                .unwrap()?;
            let protective_reads = storage
                .storage_logs_dedup_dal()
                .get_protective_reads_for_l1_batch(l1_batch_number)
                .await;
            let touched_slots = storage
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(l1_batch_number)
                .await
                .unwrap();

            let mut storage_logs = BTreeMap::new();

            let hashed_keys: Vec<_> = protective_reads
                .iter()
                .chain(touched_slots.keys())
                .map(StorageKey::hashed_key)
                .collect();
            let previous_values = storage
                .storage_logs_dal()
                .get_previous_storage_values(&hashed_keys, l1_batch_number)
                .await
                .unwrap();
            let l1_batches_for_initial_writes = storage
                .storage_logs_dal()
                .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
                .await
                .unwrap();

            for storage_key in protective_reads {
                let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
                // Sanity check: value must not change for slots that require protective reads.
                if let Some(value) = touched_slots.get(&storage_key) {
                    assert_eq!(
                        previous_value, *value,
                        "Value was changed for slot that requires protective read"
                    );
                }

                storage_logs.insert(storage_key, TreeInstruction::Read(storage_key));
            }

            for (storage_key, value) in touched_slots {
                let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
                if previous_value != value {
                    let (_, leaf_index) = l1_batches_for_initial_writes[&storage_key.hashed_key()];
                    storage_logs.insert(
                        storage_key,
                        TreeInstruction::write(storage_key, leaf_index, value),
                    );
                }
            }

            Some(Self {
                header,
                storage_logs: storage_logs.into_values().collect(),
            })
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        insert_genesis_batch(
            &mut pool.connection().await.unwrap(),
            &GenesisParams::mock(),
        )
        .await
        .unwrap();
        reset_db_state(&pool, 5).await;

        let mut storage = pool.connection().await.unwrap();
        for l1_batch_number in 0..=5 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            let batch_with_logs = L1BatchWithLogs::new(&mut storage, l1_batch_number)
                .await
                .unwrap();
            let slow_batch_with_logs = L1BatchWithLogs::slow(&mut storage, l1_batch_number)
                .await
                .unwrap();
            assert_eq!(batch_with_logs, slow_batch_with_logs);
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_zero_no_op_logs() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..200, 2);
        for log in &mut logs[0] {
            log.value = H256::zero();
        }
        for log in logs[1].iter_mut().step_by(3) {
            log.value = H256::zero();
        }
        extend_db_state(&mut storage, logs).await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for number in 0..3 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(number)).await;
        }
    }

    async fn create_tree(temp_dir: &TempDir) -> AsyncTree {
        let db = create_db(
            temp_dir.path().to_owned(),
            0,
            16 << 20,       // 16 MiB,
            Duration::ZERO, // writes should never be stalled in tests
            500,
        )
        .await
        .unwrap();
        AsyncTree::new(db, MerkleTreeMode::Full)
    }

    async fn assert_log_equivalence(
        storage: &mut Connection<'_, Core>,
        tree: &mut AsyncTree,
        l1_batch_number: L1BatchNumber,
    ) {
        let l1_batch_with_logs = L1BatchWithLogs::new(storage, l1_batch_number)
            .await
            .unwrap();
        let slow_l1_batch_with_logs = L1BatchWithLogs::slow(storage, l1_batch_number)
            .await
            .unwrap();

        // Sanity check: L1 batch headers must be identical
        assert_eq!(l1_batch_with_logs.header, slow_l1_batch_with_logs.header);

        tree.save().await; // Necessary for `reset()` below to work properly
        let tree_metadata = tree.process_l1_batch(l1_batch_with_logs.storage_logs).await;
        tree.as_mut().reset();
        let slow_tree_metadata = tree
            .process_l1_batch(slow_l1_batch_with_logs.storage_logs)
            .await;
        assert_eq!(tree_metadata.root_hash, slow_tree_metadata.root_hash);
        assert_eq!(
            tree_metadata.rollup_last_leaf_index,
            slow_tree_metadata.rollup_last_leaf_index
        );
        assert_eq!(
            tree_metadata.initial_writes,
            slow_tree_metadata.initial_writes
        );
        assert_eq!(
            tree_metadata.initial_writes,
            slow_tree_metadata.initial_writes
        );
        assert_eq!(
            tree_metadata.repeated_writes,
            slow_tree_metadata.repeated_writes
        );
        assert_equivalent_witnesses(
            tree_metadata.witness.unwrap(),
            slow_tree_metadata.witness.unwrap(),
        );
    }

    fn assert_equivalent_witnesses(lhs: PrepareBasicCircuitsJob, rhs: PrepareBasicCircuitsJob) {
        assert_eq!(lhs.next_enumeration_index(), rhs.next_enumeration_index());
        let lhs_paths = lhs.into_merkle_paths();
        let rhs_paths = rhs.into_merkle_paths();
        assert_eq!(lhs_paths.len(), rhs_paths.len());
        for (lhs_path, rhs_path) in lhs_paths.zip(rhs_paths) {
            assert_eq!(lhs_path, rhs_path);
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_non_zero_no_op_logs() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..120, 1);
        // Entire batch of no-op logs (writing previous values).
        let copied_logs = logs[0].clone();
        logs.push(copied_logs);

        // Batch of effectively no-op logs (overwriting values, then writing old values back).
        let mut updated_and_then_copied_logs: Vec<_> = logs[0]
            .iter()
            .map(|log| StorageLog {
                value: H256::repeat_byte(0xff),
                ..*log
            })
            .collect();
        updated_and_then_copied_logs.extend_from_slice(&logs[0]);
        logs.push(updated_and_then_copied_logs);

        // Batch where half of logs are copied and the other half is writing zero values (which is
        // not a no-op).
        let mut partially_copied_logs = logs[0].clone();
        for log in partially_copied_logs.iter_mut().step_by(2) {
            log.value = H256::zero();
        }
        logs.push(partially_copied_logs);

        // Batch where 2/3 of logs are copied and the other 1/3 is writing new non-zero values.
        let mut partially_copied_logs = logs[0].clone();
        for log in partially_copied_logs.iter_mut().step_by(3) {
            log.value = H256::repeat_byte(0x11);
        }
        logs.push(partially_copied_logs);
        extend_db_state(&mut storage, logs).await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for batch_number in 0..5 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(batch_number)).await;
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_protective_reads() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..120, 1);
        let logs_copy = logs[0].clone();
        logs.push(logs_copy);
        let read_logs: Vec<_> = logs[1]
            .iter()
            .step_by(3)
            .map(StorageLog::to_test_log_query)
            .collect();
        extend_db_state(&mut storage, logs).await;
        storage
            .storage_logs_dedup_dal()
            .insert_protective_reads(L1BatchNumber(2), &read_logs)
            .await
            .unwrap();

        let l1_batch_with_logs = L1BatchWithLogs::new(&mut storage, L1BatchNumber(2))
            .await
            .unwrap();
        // Check that we have protective reads transformed into read logs
        let read_logs_count = l1_batch_with_logs
            .storage_logs
            .iter()
            .filter(|log| matches!(log, TreeInstruction::Read(_)))
            .count();
        assert_eq!(read_logs_count, 7);

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for batch_number in 0..3 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(batch_number)).await;
        }
    }
}

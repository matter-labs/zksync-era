//! Various helpers for the metadata calculator.

use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Serialize;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_config::configs::database::MerkleTreeMode;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_health_check::{CheckHealth, Health, HealthStatus, ReactiveHealthCheck};
use zksync_merkle_tree::{
    domain::{TreeMetadata, ZkSyncTree, ZkSyncTreeReader},
    recovery::{MerkleTreeRecovery, PersistenceThreadHandle},
    repair::StaleKeysRepairTask,
    unstable::{NodeKey, RawNode},
    Database, Key, MerkleTreeColumnFamily, NoVersionError, RocksDBWrapper, TreeEntry,
    TreeEntryWithProof, TreeInstruction,
};
use zksync_shared_metrics::tree::{LoadChangesStage, TreeUpdateStage, METRICS};
use zksync_shared_resources::tree::MerkleTreeInfo;
use zksync_storage::{RocksDB, RocksDBOptions, StalledWritesRetries, WeakRocksDB};
use zksync_types::{
    block::{L1BatchStatistics, L1BatchTreeData},
    writes::TreeWrite,
    AccountTreeId, L1BatchNumber, StorageKey, H256,
};

use super::{
    pruning::PruningHandles, MerkleTreeReaderConfig, MetadataCalculatorConfig,
    MetadataCalculatorRecoveryConfig,
};

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
        let status = match &details {
            MerkleTreeHealth::Initialization | MerkleTreeHealth::Recovery { .. } => {
                HealthStatus::Affected
            }
            MerkleTreeHealth::MainLoop(_) => HealthStatus::Ready,
        };
        Self::from(status).with_details(details)
    }
}

/// Health check for the Merkle tree.
///
/// [`ReactiveHealthCheck`] is not sufficient for the tree because in the main loop, tree info
/// can be updated by multiple tasks (the metadata calculator and the pruning task). Additionally,
/// keeping track of all places where the info is updated is error-prone.
#[derive(Debug)]
pub(super) struct MerkleTreeHealthCheck {
    reactive_check: ReactiveHealthCheck,
    weak_reader: Arc<OnceCell<WeakAsyncTreeReader>>,
}

impl MerkleTreeHealthCheck {
    pub fn new(reactive_check: ReactiveHealthCheck, reader: LazyAsyncTreeReader) -> Self {
        // We must not retain a strong RocksDB ref in the health check because it will prevent
        // proper node shutdown (which waits until all RocksDB instances are dropped); health checks
        // are dropped after all components are terminated.
        let weak_reader = Arc::<OnceCell<WeakAsyncTreeReader>>::default();
        let weak_reader_for_task = weak_reader.clone();
        tokio::spawn(async move {
            if let Some(reader) = reader.wait().await {
                weak_reader_for_task.set(reader.downgrade()).ok();
            }
            // Otherwise, the tree is dropped before getting initialized; this is not an error in this context.
        });

        Self {
            reactive_check,
            weak_reader,
        }
    }
}

#[async_trait]
impl CheckHealth for MerkleTreeHealthCheck {
    fn name(&self) -> &'static str {
        "tree"
    }

    async fn check_health(&self) -> Health {
        let health = self.reactive_check.check_health().await;
        if !matches!(health.status(), HealthStatus::Ready) {
            return health;
        }

        if let Some(reader) = self
            .weak_reader
            .get()
            .and_then(WeakAsyncTreeReader::upgrade)
        {
            let info = reader.info().await;
            health.with_details(MerkleTreeHealth::MainLoop(info))
        } else {
            health
        }
    }
}

/// Creates a RocksDB wrapper with the specified params.
pub(super) async fn create_db(config: MetadataCalculatorConfig) -> anyhow::Result<RocksDBWrapper> {
    tokio::task::spawn_blocking(move || create_db_sync(&config))
        .await
        .context("panicked creating Merkle tree RocksDB")?
}

fn create_db_sync(config: &MetadataCalculatorConfig) -> anyhow::Result<RocksDBWrapper> {
    let &MetadataCalculatorConfig {
        max_open_files,
        block_cache_capacity,
        include_indices_and_filters_in_block_cache,
        multi_get_chunk_size,
        memtable_capacity,
        stalled_writes_timeout,
        ..
    } = config;

    tracing::info!(
        "Initializing Merkle tree database at `{path}` (max open files: {max_open_files:?}) with {multi_get_chunk_size} multi-get chunk size, \
         {block_cache_capacity}B block cache (indices & filters included: {include_indices_and_filters_in_block_cache:?}), \
         {memtable_capacity}B memtable capacity, \
         {stalled_writes_timeout:?} stalled writes timeout",
        path = config.db_path.display()
    );

    let mut db = RocksDB::with_options(
        &config.db_path,
        RocksDBOptions {
            block_cache_capacity: Some(block_cache_capacity),
            include_indices_and_filters_in_block_cache,
            large_memtable_capacity: Some(memtable_capacity),
            stalled_writes_retries: StalledWritesRetries::new(stalled_writes_timeout),
            max_open_files,
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

pub(super) async fn create_readonly_db(
    config: MerkleTreeReaderConfig,
) -> anyhow::Result<RocksDBWrapper> {
    tokio::task::spawn_blocking(move || {
        let MerkleTreeReaderConfig {
            db_path,
            max_open_files,
            multi_get_chunk_size,
            block_cache_capacity,
            include_indices_and_filters_in_block_cache,
        } = config;

        tracing::info!(
            "Initializing Merkle tree database at `{db_path:?}` (max open files: {max_open_files:?}) with {multi_get_chunk_size} multi-get chunk size, \
             {block_cache_capacity}B block cache (indices & filters included: {include_indices_and_filters_in_block_cache:?})"
        );
        let mut db = RocksDB::with_options(
            db_path.as_ref(),
            RocksDBOptions {
                block_cache_capacity: Some(block_cache_capacity),
                include_indices_and_filters_in_block_cache,
                max_open_files,
                ..RocksDBOptions::default()
            }
        )?;
        if cfg!(test) {
            db = db.with_sync_writes();
        }
        Ok(RocksDBWrapper::from(db))
    })
    .await
    .context("panicked creating Merkle tree RocksDB")?
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
        "`AsyncTree` is in inconsistent state, which could occur after one of its async methods was cancelled or returned an error";

    pub fn new(db: RocksDBWrapper, mode: MerkleTreeMode) -> anyhow::Result<Self> {
        let tree = match mode {
            MerkleTreeMode::Full => ZkSyncTree::new(db),
            MerkleTreeMode::Lightweight => ZkSyncTree::new_lightweight(db),
        }?;
        Ok(Self {
            inner: Some(tree),
            mode,
        })
    }

    fn as_ref(&self) -> &ZkSyncTree {
        self.inner.as_ref().expect(Self::INCONSISTENT_MSG)
    }

    fn as_mut(&mut self) -> &mut ZkSyncTree {
        self.inner.as_mut().expect(Self::INCONSISTENT_MSG)
    }

    pub fn mode(&self) -> MerkleTreeMode {
        self.mode
    }

    pub fn pruner(&mut self) -> PruningHandles {
        self.as_mut().pruner()
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

    pub fn min_l1_batch_number(&self) -> Option<L1BatchNumber> {
        self.as_ref().reader().min_l1_batch_number()
    }

    #[cfg(test)]
    pub fn root_hash(&self) -> H256 {
        self.as_ref().root_hash()
    }

    pub fn data_for_l1_batch(&self, l1_batch_number: L1BatchNumber) -> Option<L1BatchTreeData> {
        let (hash, leaf_count) = self.as_ref().root_info(l1_batch_number)?;
        Some(L1BatchTreeData {
            hash,
            rollup_last_leaf_index: leaf_count + 1,
        })
    }

    /// Returned errors are unrecoverable; the tree must not be used after an error is returned.
    pub async fn process_l1_batch(
        &mut self,
        batch: L1BatchWithLogs,
    ) -> anyhow::Result<TreeMetadata> {
        anyhow::ensure!(
            batch.mode == self.mode,
            "Cannot process L1 batch with mode {:?} in tree with mode {:?}",
            batch.mode,
            self.mode
        );
        let batch_number = batch.stats.number;

        let mut tree = self.inner.take().context(Self::INCONSISTENT_MSG)?;
        let (tree, metadata) = tokio::task::spawn_blocking(move || {
            let metadata = tree.process_l1_batch(&batch.storage_logs)?;
            anyhow::Ok((tree, metadata))
        })
        .await
        .with_context(|| {
            format!("Merkle tree panicked when processing L1 batch #{batch_number}")
        })??;

        self.inner = Some(tree);
        Ok(metadata)
    }

    /// Returned errors are unrecoverable; the tree must not be used after an error is returned.
    pub async fn save(&mut self) -> anyhow::Result<()> {
        let mut tree = self.inner.take().context(Self::INCONSISTENT_MSG)?;
        self.inner = Some(
            tokio::task::spawn_blocking(|| {
                tree.save()?;
                anyhow::Ok(tree)
            })
            .await
            .context("Merkle tree panicked during saving")??,
        );
        Ok(())
    }

    pub fn roll_back_logs(&mut self, last_l1_batch_to_keep: L1BatchNumber) -> anyhow::Result<()> {
        self.as_mut().roll_back_logs(last_l1_batch_to_keep)
    }
}

/// Async version of [`ZkSyncTreeReader`].
#[derive(Debug, Clone)]
pub struct AsyncTreeReader {
    inner: ZkSyncTreeReader,
    mode: MerkleTreeMode,
}

impl AsyncTreeReader {
    pub(super) fn new(db: RocksDBWrapper, mode: MerkleTreeMode) -> anyhow::Result<Self> {
        Ok(Self {
            inner: ZkSyncTreeReader::new(db)?,
            mode,
        })
    }

    fn downgrade(&self) -> WeakAsyncTreeReader {
        WeakAsyncTreeReader {
            db: self.inner.db().clone().into_inner().downgrade(),
            mode: self.mode,
        }
    }

    pub async fn info(self) -> MerkleTreeInfo {
        tokio::task::spawn_blocking(move || {
            loop {
                let next_l1_batch_number = self.inner.next_l1_batch_number();
                let latest_l1_batch_number = next_l1_batch_number.checked_sub(1);
                let root_info = if let Some(number) = latest_l1_batch_number {
                    self.inner.root_info(L1BatchNumber(number))
                } else {
                    // No L1 batches in the tree yet.
                    Some((ZkSyncTree::empty_tree_hash(), 0))
                };
                let Some((root_hash, leaf_count)) = root_info else {
                    // It is possible (although very unlikely) that the latest tree version was removed after requesting it,
                    // hence the outer loop; RocksDB doesn't provide consistent data views by default.
                    tracing::info!(
                        "Tree version at L1 batch {latest_l1_batch_number:?} was removed after requesting the latest tree L1 batch; \
                         re-requesting tree information"
                    );
                    continue;
                };

                // `min_l1_batch_number` is not necessarily consistent with other retrieved tree data, but this looks fine.
                break MerkleTreeInfo {
                    mode: self.mode,
                    root_hash,
                    next_l1_batch_number,
                    min_l1_batch_number: self.inner.min_l1_batch_number(),
                    leaf_count,
                };
            }
        })
        .await
        .unwrap()
    }

    #[cfg(test)]
    pub async fn verify_consistency(self, l1_batch_number: L1BatchNumber) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || self.inner.verify_consistency(l1_batch_number))
            .await
            .context("tree consistency verification panicked")?
            .map_err(Into::into)
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

    pub(crate) async fn raw_nodes(self, keys: Vec<NodeKey>) -> Vec<Option<RawNode>> {
        tokio::task::spawn_blocking(move || self.inner.raw_nodes(&keys))
            .await
            .unwrap()
    }

    pub(crate) async fn raw_stale_keys(self, l1_batch_number: L1BatchNumber) -> Vec<NodeKey> {
        tokio::task::spawn_blocking(move || self.inner.raw_stale_keys(l1_batch_number))
            .await
            .unwrap()
    }

    pub(crate) async fn bogus_stale_keys(self, l1_batch_number: L1BatchNumber) -> Vec<NodeKey> {
        let version = l1_batch_number.0.into();
        tokio::task::spawn_blocking(move || {
            StaleKeysRepairTask::bogus_stale_keys(self.inner.db(), version)
        })
        .await
        .unwrap()
    }

    pub(crate) fn into_db(self) -> RocksDBWrapper {
        self.inner.into_db()
    }
}

/// Version of async tree reader that holds a weak reference to RocksDB. Used in [`MerkleTreeHealthCheck`].
#[derive(Debug)]
struct WeakAsyncTreeReader {
    db: WeakRocksDB<MerkleTreeColumnFamily>,
    mode: MerkleTreeMode,
}

impl WeakAsyncTreeReader {
    fn upgrade(&self) -> Option<AsyncTreeReader> {
        Some(AsyncTreeReader {
            inner: ZkSyncTreeReader::new(self.db.upgrade()?.into()).ok()?,
            mode: self.mode,
        })
    }
}

/// Lazily initialized [`AsyncTreeReader`].
#[derive(Debug)]
pub struct LazyAsyncTreeReader(pub(super) watch::Receiver<Option<AsyncTreeReader>>);

impl LazyAsyncTreeReader {
    /// Returns a reader if it is initialized.
    pub fn read(&self) -> Option<AsyncTreeReader> {
        self.0.borrow().clone()
    }

    /// Waits until the tree is initialized and returns a reader for it. If the tree is dropped before
    /// getting initialized, returns `None`.
    pub async fn wait(mut self) -> Option<AsyncTreeReader> {
        loop {
            if let Some(reader) = self.0.borrow().clone() {
                break Some(reader);
            }
            self.0.changed().await.ok()?;
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

    pub fn new(
        db: RocksDBWrapper,
        recovered_version: u64,
        mode: MerkleTreeMode,
        config: &MetadataCalculatorRecoveryConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self::with_handle(db, recovered_version, mode, config)?.0)
    }

    // Public for testing purposes
    pub fn with_handle(
        db: RocksDBWrapper,
        recovered_version: u64,
        mode: MerkleTreeMode,
        config: &MetadataCalculatorRecoveryConfig,
    ) -> anyhow::Result<(Self, Option<PersistenceThreadHandle>)> {
        let mut recovery = MerkleTreeRecovery::new(db, recovered_version)?;
        let handle = config
            .parallel_persistence_buffer
            .map(|buffer_capacity| recovery.parallelize_persistence(buffer_capacity.get()))
            .transpose()?;
        let this = Self {
            inner: Some(recovery),
            mode,
        };
        Ok((this, handle))
    }

    pub fn recovered_version(&self) -> u64 {
        self.inner
            .as_ref()
            .expect(Self::INCONSISTENT_MSG)
            .recovered_version()
    }

    pub async fn ensure_desired_chunk_size(
        &mut self,
        desired_chunk_size: u64,
    ) -> anyhow::Result<()> {
        const CHUNK_SIZE_KEY: &str = "recovery.desired_chunk_size";

        let mut tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let tree = tokio::task::spawn_blocking(move || {
            // **Important.** Tags should not be mutated on error (i.e., it would be an error to unconditionally call `tags.insert()`
            // and then check the previous value).
            tree.update_custom_tags(|tags| {
                if let Some(chunk_size_in_tree) = tags.get(CHUNK_SIZE_KEY) {
                    let chunk_size_in_tree: u64 = chunk_size_in_tree
                        .parse()
                        .with_context(|| format!("error parsing desired_chunk_size `{chunk_size_in_tree}` in Merkle tree tags"))?;
                    anyhow::ensure!(
                        chunk_size_in_tree == desired_chunk_size,
                        "Mismatch between the configured desired chunk size ({desired_chunk_size}) and one that was used previously ({chunk_size_in_tree}). \
                         Either change the desired chunk size in configuration, or reset Merkle tree recovery by clearing its RocksDB directory"
                    );
                } else {
                    tags.insert(CHUNK_SIZE_KEY.to_owned(), desired_chunk_size.to_string());
                }
                Ok(())
            })
            .context("failed updating Merkle tree tags")??;
            anyhow::Ok(tree)
        })
        .await??;

        self.inner = Some(tree);
        Ok(())
    }

    /// Returns an entry for the specified keys.
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
    pub async fn extend(&mut self, entries: Vec<TreeEntry>) -> anyhow::Result<()> {
        let mut tree = self.inner.take().expect(Self::INCONSISTENT_MSG);
        let tree = tokio::task::spawn_blocking(move || {
            tree.extend_random(entries)?;
            anyhow::Ok(tree)
        })
        .await
        .context("extending tree with recovery entries panicked")??;

        self.inner = Some(tree);
        Ok(())
    }

    /// Waits until all pending chunks are persisted.
    pub async fn wait_for_persistence(self) -> anyhow::Result<()> {
        let tree = self.inner.expect(Self::INCONSISTENT_MSG);
        tokio::task::spawn_blocking(|| tree.wait_for_persistence())
            .await
            .context("panicked while waiting for pending recovery chunks to be persisted")?
    }

    pub async fn finalize(self) -> anyhow::Result<AsyncTree> {
        let tree = self.inner.expect(Self::INCONSISTENT_MSG);
        let db = tokio::task::spawn_blocking(|| tree.finalize())
            .await
            .context("finalizing tree panicked")??;
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
    pub async fn new(
        db: RocksDBWrapper,
        config: &MetadataCalculatorConfig,
    ) -> anyhow::Result<Self> {
        let mode = config.mode;
        let recovery = config.recovery.clone();
        tokio::task::spawn_blocking(move || {
            let Some(manifest) = db.manifest() else {
                return Ok(Self::Empty { db, mode });
            };
            anyhow::Ok(if let Some(version) = manifest.recovered_version() {
                Self::Recovering(AsyncTreeRecovery::new(db, version, mode, &recovery)?)
            } else {
                Self::Ready(AsyncTree::new(db, mode)?)
            })
        })
        .await
        .context("loading Merkle tree panicked")?
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
    pub stats: L1BatchStatistics,
    pub storage_logs: Vec<TreeInstruction>,
    mode: MerkleTreeMode,
}

impl L1BatchWithLogs {
    pub async fn new(
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        mode: MerkleTreeMode,
    ) -> anyhow::Result<Option<Self>> {
        tracing::info!(
            "Loading storage logs data for L1 batch #{l1_batch_number} for {mode:?} tree"
        );
        let load_changes_latency = METRICS.start_stage(TreeUpdateStage::LoadChanges);

        let header_latency = METRICS.start_load_stage(LoadChangesStage::LoadL1BatchHeader);
        let Some(header) = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .context("cannot fetch L1 batch header")?
        else {
            return Ok(None);
        };
        println!("header: {:?}", header);
        let stats = header.into();
        println!("stats: {:?}", stats);
        header_latency.observe();

        let protective_reads = match mode {
            MerkleTreeMode::Full => {
                let protective_reads_latency =
                    METRICS.start_load_stage(LoadChangesStage::LoadProtectiveReads);
                let protective_reads = storage
                    .storage_logs_dedup_dal()
                    .get_protective_reads_for_l1_batch(l1_batch_number)
                    .await?;
                if protective_reads.is_empty() {
                    tracing::warn!(
                        "Protective reads for L1 batch #{l1_batch_number} are empty. This is highly unlikely \
                         and could be caused by disabling protective reads persistence in state keeper"
                    );
                }
                protective_reads_latency.observe_with_count(protective_reads.len());
                protective_reads
            }
            MerkleTreeMode::Lightweight => HashSet::new(),
        };

        let load_tree_writes_latency = METRICS.start_load_stage(LoadChangesStage::LoadTreeWrites);
        let mut tree_writes = storage
            .blocks_dal()
            .get_tree_writes(l1_batch_number)
            .await?;
        if tree_writes.is_none() && l1_batch_number.0 > 0 {
            // If `tree_writes` are present for the previous L1 batch, then it is expected them to be eventually present for the current batch as well.
            // Waiting for tree writes should be faster then constructing them, so we wait with a reasonable timeout.
            let tree_writes_present_for_previous_batch = storage
                .blocks_dal()
                .check_tree_writes_presence(l1_batch_number - 1)
                .await?;
            if tree_writes_present_for_previous_batch {
                tree_writes = Self::wait_for_tree_writes(storage, l1_batch_number).await?;
            }
        }
        println!(
            "number of tree_writes: {:?}",
            tree_writes.as_ref().map(|tw| tw.len())
        );
        println!("tree_writes: {:?}", tree_writes);
        load_tree_writes_latency.observe();

        let storage_logs = if let Some(tree_writes) = tree_writes {
            // If tree writes are present in DB then simply use them.
            let writes = tree_writes.into_iter().map(|tree_write| {
                let storage_key =
                    StorageKey::new(AccountTreeId::new(tree_write.address), tree_write.key);
                TreeInstruction::write(storage_key, tree_write.leaf_index, tree_write.value)
            });
            let reads = protective_reads.into_iter().map(TreeInstruction::Read);

            writes
                .chain(reads)
                .sorted_by_key(|tree_instruction| tree_instruction.key())
                .map(TreeInstruction::with_hashed_key)
                .collect()
        } else {
            // Otherwise, load writes' data from other tables.
            Self::extract_storage_logs_from_db(storage, l1_batch_number, protective_reads).await?
        };
        println!("storage_logs: {:?}", storage_logs);

        load_changes_latency.observe();

        Ok(Some(Self {
            stats,
            storage_logs,
            mode,
        }))
    }

    async fn wait_for_tree_writes(
        connection: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<TreeWrite>>> {
        const INTERVAL: Duration = Duration::from_millis(50);
        const TIMEOUT: Duration = Duration::from_secs(5);

        tokio::time::timeout(TIMEOUT, async {
            loop {
                if let Some(tree_writes) = connection
                    .blocks_dal()
                    .get_tree_writes(l1_batch_number)
                    .await?
                {
                    break anyhow::Ok(tree_writes);
                }
                tokio::time::sleep(INTERVAL).await;
            }
        })
        .await
        .ok()
        .transpose()
    }

    async fn extract_storage_logs_from_db(
        connection: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        protective_reads: HashSet<StorageKey>,
    ) -> anyhow::Result<Vec<TreeInstruction>> {
        let touched_slots_latency = METRICS.start_load_stage(LoadChangesStage::LoadTouchedSlots);
        let mut touched_slots = connection
            .storage_logs_dal()
            .get_touched_slots_for_executed_l1_batch(l1_batch_number)
            .await
            .context("cannot fetch touched slots")?;
        touched_slots_latency.observe_with_count(touched_slots.len());

        let leaf_indices_latency = METRICS.start_load_stage(LoadChangesStage::LoadLeafIndices);
        let hashed_keys_for_writes: Vec<_> =
            touched_slots.keys().map(StorageKey::hashed_key).collect();
        let l1_batches_for_initial_writes = connection
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&hashed_keys_for_writes)
            .await
            .context("cannot fetch initial writes batch numbers and indices")?;
        leaf_indices_latency.observe_with_count(hashed_keys_for_writes.len());

        let mut storage_logs = BTreeMap::new();
        for storage_key in protective_reads {
            touched_slots.remove(&storage_key);
            // ^ As per deduplication rules, all keys in `protective_reads` haven't *really* changed
            // in the considered L1 batch. Thus, we can remove them from `touched_slots` in order to simplify
            // their further processing. This is not a required step; the logic below works fine without it.
            // Indeed, extra no-op updates that could be added to `storage_logs` as a consequence of no filtering,
            // are removed on the Merkle tree level (see the tree domain wrapper).
            let log = TreeInstruction::Read(storage_key.hashed_key_u256());
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
                        TreeInstruction::write(storage_key.hashed_key_u256(), leaf_index, value),
                    );
                }
            }
        }

        Ok(storage_logs.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_prover_interface::inputs::WitnessInputMerklePaths;
    use zksync_types::{writes::TreeWrite, StorageKey, StorageLog, U256};

    use super::*;
    use crate::tests::{extend_db_state, gen_storage_logs, mock_config, reset_db_state};

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
                .await
                .unwrap();
            let touched_slots = storage
                .storage_logs_dal()
                .get_touched_slots_for_executed_l1_batch(l1_batch_number)
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

                storage_logs.insert(
                    storage_key,
                    TreeInstruction::Read(storage_key.hashed_key_u256()),
                );
            }

            for (storage_key, value) in touched_slots {
                let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
                if previous_value != value {
                    let (_, leaf_index) = l1_batches_for_initial_writes[&storage_key.hashed_key()];
                    storage_logs.insert(
                        storage_key,
                        TreeInstruction::write(storage_key.hashed_key_u256(), leaf_index, value),
                    );
                }
            }

            Some(Self {
                stats: header.into(),
                storage_logs: storage_logs.into_values().collect(),
                mode: MerkleTreeMode::Full,
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
        let mut tree_writes = Vec::new();

        // Create a lookup table for storage key preimages
        let all_storage_logs = storage
            .storage_logs_dal()
            .dump_all_storage_logs_for_tests()
            .await;
        let logs_by_hashed_key: HashMap<_, _> = all_storage_logs
            .into_iter()
            .map(|log| {
                let tree_key = U256::from_little_endian(log.hashed_key.as_bytes());
                (tree_key, log)
            })
            .collect();

        // Check equivalence in case `tree_writes` are not present in DB.
        for l1_batch_number in 0..=5 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            let batch_with_logs =
                L1BatchWithLogs::new(&mut storage, l1_batch_number, MerkleTreeMode::Full)
                    .await
                    .unwrap()
                    .expect("no L1 batch");
            let slow_batch_with_logs = L1BatchWithLogs::slow(&mut storage, l1_batch_number)
                .await
                .unwrap();
            assert_eq!(batch_with_logs, slow_batch_with_logs);

            let writes = batch_with_logs
                .storage_logs
                .into_iter()
                .filter_map(|instruction| match instruction {
                    TreeInstruction::Write(tree_entry) => Some(TreeWrite {
                        address: logs_by_hashed_key[&tree_entry.key].address.unwrap(),
                        key: logs_by_hashed_key[&tree_entry.key].key.unwrap(),
                        value: tree_entry.value,
                        leaf_index: tree_entry.leaf_index,
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            tree_writes.push(writes);
        }

        // Insert `tree_writes` and check again.
        for l1_batch_number in 0..5 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            storage
                .blocks_dal()
                .set_tree_writes(
                    l1_batch_number,
                    tree_writes[l1_batch_number.0 as usize].clone(),
                )
                .await
                .unwrap();

            let batch_with_logs =
                L1BatchWithLogs::new(&mut storage, l1_batch_number, MerkleTreeMode::Full)
                    .await
                    .unwrap()
                    .expect("no L1 batch");
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
        let db = create_db(mock_config(temp_dir.path())).await.unwrap();
        AsyncTree::new(db, MerkleTreeMode::Full).unwrap()
    }

    async fn assert_log_equivalence(
        storage: &mut Connection<'_, Core>,
        tree: &mut AsyncTree,
        l1_batch_number: L1BatchNumber,
    ) {
        let l1_batch_with_logs =
            L1BatchWithLogs::new(storage, l1_batch_number, MerkleTreeMode::Full)
                .await
                .unwrap()
                .expect("no L1 batch");
        let mut lightweight_l1_batch_with_logs =
            L1BatchWithLogs::new(storage, l1_batch_number, MerkleTreeMode::Lightweight)
                .await
                .unwrap()
                .expect("no L1 batch");
        let slow_l1_batch_with_logs = L1BatchWithLogs::slow(storage, l1_batch_number)
            .await
            .unwrap();

        // Sanity check: L1 batch headers must be identical
        assert_eq!(l1_batch_with_logs.stats, slow_l1_batch_with_logs.stats);
        assert_eq!(
            lightweight_l1_batch_with_logs.stats,
            slow_l1_batch_with_logs.stats
        );

        tree.save().await.unwrap(); // Necessary for `reset()` below to work properly
        let tree_metadata = tree.process_l1_batch(l1_batch_with_logs).await.unwrap();
        tree.as_mut().reset();
        lightweight_l1_batch_with_logs.mode = tree.mode; // Manually override the mode so that processing won't panic
        let lightweight_tree_metadata = tree
            .process_l1_batch(lightweight_l1_batch_with_logs)
            .await
            .unwrap();
        tree.as_mut().reset();
        let slow_tree_metadata = tree
            .process_l1_batch(slow_l1_batch_with_logs)
            .await
            .unwrap();
        assert_metadata_eq(&tree_metadata, &slow_tree_metadata);
        assert_metadata_eq(&lightweight_tree_metadata, &slow_tree_metadata);
        assert_equivalent_witnesses(
            tree_metadata.witness.unwrap(),
            slow_tree_metadata.witness.unwrap(),
        );
    }

    fn assert_metadata_eq(actual: &TreeMetadata, expected: &TreeMetadata) {
        assert_eq!(actual.root_hash, expected.root_hash);
        assert_eq!(
            actual.rollup_last_leaf_index,
            expected.rollup_last_leaf_index
        );
    }

    fn assert_equivalent_witnesses(lhs: WitnessInputMerklePaths, rhs: WitnessInputMerklePaths) {
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
        let read_logs: Vec<_> = logs[1].iter().step_by(3).cloned().collect();
        extend_db_state(&mut storage, logs).await;
        storage
            .storage_logs_dedup_dal()
            .insert_protective_reads(L1BatchNumber(2), &read_logs)
            .await
            .unwrap();

        let l1_batch_with_logs =
            L1BatchWithLogs::new(&mut storage, L1BatchNumber(2), MerkleTreeMode::Full)
                .await
                .unwrap()
                .expect("no L1 batch");
        // Check that we have protective reads transformed into read logs
        let read_logs_count = l1_batch_with_logs
            .storage_logs
            .iter()
            .filter(|log| matches!(log, TreeInstruction::Read(_)))
            .count();
        assert_eq!(read_logs_count, 7);

        let light_l1_batch_with_logs =
            L1BatchWithLogs::new(&mut storage, L1BatchNumber(2), MerkleTreeMode::Lightweight)
                .await
                .unwrap()
                .expect("no L1 batch");
        assert!(light_l1_batch_with_logs
            .storage_logs
            .iter()
            .all(|log| matches!(log, TreeInstruction::Write(_))));
        // Check that write instructions are equivalent for the full and light L1 batches (light logs may include extra no-op writes).
        let write_logs: HashSet<_> = l1_batch_with_logs
            .storage_logs
            .into_iter()
            .filter(|log| matches!(log, TreeInstruction::Write(_)))
            .collect();
        let light_write_logs: HashSet<_> =
            light_l1_batch_with_logs.storage_logs.into_iter().collect();
        assert!(
            light_write_logs.is_superset(&write_logs),
            "full={write_logs:?}, light={light_write_logs:?}"
        );

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for batch_number in 0..3 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(batch_number)).await;
        }
    }
}

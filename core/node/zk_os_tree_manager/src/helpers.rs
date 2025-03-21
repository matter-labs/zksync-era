use anyhow::Context as _;
use tokio::sync::watch;
use zk_os_merkle_tree::{
    unstable, BatchTreeProof, MerkleTree, MerkleTreeColumnFamily, MerkleTreeReader, Patched,
    RocksDBWrapper,
};
use zksync_storage::{RocksDB, RocksDBOptions, StalledWritesRetries, WeakRocksDB};
use zksync_types::{block::L1BatchTreeData, L1BatchNumber, H256};

use crate::{health::MerkleTreeInfo, TreeManagerConfig};

/// Async version of [`ZkSyncTreeReader`].
#[derive(Debug, Clone)]
pub struct AsyncTreeReader {
    inner: MerkleTreeReader<RocksDBWrapper>,
}

impl AsyncTreeReader {
    pub(crate) fn downgrade(&self) -> WeakAsyncTreeReader {
        WeakAsyncTreeReader {
            db: self.inner.db().clone().into_inner().downgrade(),
        }
    }

    pub async fn info(self) -> anyhow::Result<MerkleTreeInfo> {
        tokio::task::spawn_blocking(move || {
            loop {
                let latest_version = self.inner.latest_version()?;
                let root_info = if let Some(version) = latest_version {
                    self.inner.root_info(version)?
                } else {
                    // No versions in the tree yet.
                    Some((PatchedMerkleTree::empty_tree_hash(), 0))
                };

                let Some((root_hash, leaf_count)) = root_info else {
                    // It is possible (although very unlikely) that the latest tree version was removed after requesting it,
                    // hence the outer loop; RocksDB doesn't provide consistent data views by default.
                    tracing::info!(
                        "Tree version at L1 batch {latest_version:?} was removed after requesting the latest tree L1 batch; \
                         re-requesting tree information"
                    );
                    continue;
                };

                // `min_l1_batch_number` is not necessarily consistent with other retrieved tree data, but this looks fine.
                break Ok(MerkleTreeInfo {
                    root_hash,
                    next_version: latest_version.map_or(0, |ver| ver + 1),
                    // Valid because pruning isn't implemented yet
                    min_version: latest_version.is_some().then_some(0),
                    leaf_count,
                });
            }
        })
        .await
        .unwrap()
    }

    pub(crate) async fn prove(
        self,
        version: u64,
        keys: Vec<H256>,
    ) -> anyhow::Result<BatchTreeProof> {
        tokio::task::spawn_blocking(move || self.inner.prove(version, &keys))
            .await
            .context("getting proof panicked")?
    }

    pub(crate) async fn raw_nodes(
        self,
        keys: Vec<unstable::NodeKey>,
    ) -> anyhow::Result<Vec<Option<unstable::RawNode>>> {
        tokio::task::spawn_blocking(move || self.inner.raw_nodes(&keys))
            .await
            .context("getting raw nodes panicked")?
    }
}

/// Version of async tree reader that holds a weak reference to RocksDB. Used in [`MerkleTreeHealthCheck`].
#[derive(Debug)]
pub(crate) struct WeakAsyncTreeReader {
    db: WeakRocksDB<MerkleTreeColumnFamily>,
}

impl WeakAsyncTreeReader {
    pub(crate) fn upgrade(&self) -> Option<AsyncTreeReader> {
        Some(AsyncTreeReader {
            inner: MerkleTreeReader::new(self.db.upgrade()?.into()).ok()?,
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

type PatchedMerkleTree = MerkleTree<Patched<RocksDBWrapper>>;

/// Wrapper around the "main" tree implementation used by [`MetadataCalculator`].
///
/// Async methods provided by this wrapper are not cancel-safe!
#[derive(Debug)]
pub(crate) struct AsyncMerkleTree {
    inner: Option<PatchedMerkleTree>,
}

impl AsyncMerkleTree {
    const INCONSISTENT_MSG: &'static str =
        "`AsyncMerkleTree` is in inconsistent state, which could occur after one of its async methods was cancelled or returned an error";

    pub(crate) async fn new(db: RocksDBWrapper) -> anyhow::Result<Self> {
        tokio::task::spawn_blocking(|| {
            let tree = MerkleTree::new(Patched::new(db))?;
            Ok(Self { inner: Some(tree) })
        })
        .await
        .context("panicked creating Merkle tree")?
    }

    pub(crate) fn reader(&self) -> AsyncTreeReader {
        let db = self
            .inner
            .as_ref()
            .expect(Self::INCONSISTENT_MSG)
            .db()
            .inner()
            .clone();
        AsyncTreeReader {
            inner: MerkleTreeReader::new(db)
                .expect("failed consistency checks after successfully performing them for tree"),
        }
    }

    async fn invoke_tree<T, F>(&mut self, f: F) -> anyhow::Result<T>
    where
        T: 'static + Send,
        F: FnOnce(&mut PatchedMerkleTree) -> T + 'static + Send,
    {
        let mut tree = self.inner.take().context(Self::INCONSISTENT_MSG)?;
        let (output, tree) = tokio::task::spawn_blocking(|| (f(&mut tree), tree))
            .await
            .context("tree action panicked")?;
        self.inner = Some(tree);
        Ok(output)
    }

    pub(crate) async fn try_invoke_tree<T, F>(&mut self, f: F) -> anyhow::Result<T>
    where
        T: 'static + Send,
        F: FnOnce(&mut PatchedMerkleTree) -> anyhow::Result<T> + 'static + Send,
    {
        self.invoke_tree(f).await?
    }

    pub(crate) async fn is_empty(&mut self) -> anyhow::Result<bool> {
        self.try_invoke_tree(|tree| tree.latest_version().map(|ver| ver.is_none()))
            .await
    }

    pub(crate) async fn next_l1_batch_number(&mut self) -> anyhow::Result<L1BatchNumber> {
        self.try_invoke_tree(|tree| {
            let next_version = tree.latest_version()?.map_or(0, |ver| ver + 1);
            Ok(L1BatchNumber(
                next_version.try_into().context("tree version overflow")?,
            ))
        })
        .await
    }

    pub(crate) async fn data_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<L1BatchTreeData>> {
        self.try_invoke_tree(move |tree| {
            Ok(tree
                .root_info(l1_batch_number.0.into())?
                .map(|(hash, leaf_count)| L1BatchTreeData {
                    hash,
                    rollup_last_leaf_index: leaf_count + 1,
                }))
        })
        .await
    }

    pub(crate) async fn min_l1_batch_number(&mut self) -> anyhow::Result<Option<L1BatchNumber>> {
        Ok(if self.is_empty().await? {
            None
        } else {
            // TODO: change once tree pruning is implemented
            Some(L1BatchNumber(0))
        })
    }

    pub(crate) async fn save(&mut self) -> anyhow::Result<()> {
        self.try_invoke_tree(PatchedMerkleTree::flush).await
    }

    pub(crate) async fn roll_back_logs(
        &mut self,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let retained_version_count = u64::from(last_l1_batch_to_keep.0 + 1);
        self.try_invoke_tree(move |tree| tree.truncate_recent_versions(retained_version_count))
            .await
    }
}

/// Creates a RocksDB wrapper with the specified params.
pub(crate) async fn create_db(config: TreeManagerConfig) -> anyhow::Result<RocksDBWrapper> {
    tokio::task::spawn_blocking(move || create_db_sync(&config))
        .await
        .context("panicked creating Merkle tree RocksDB")?
}

fn create_db_sync(config: &TreeManagerConfig) -> anyhow::Result<RocksDBWrapper> {
    let path = &config.db_path;
    let &TreeManagerConfig {
        max_open_files,
        block_cache_capacity,
        include_indices_and_filters_in_block_cache,
        multi_get_chunk_size,
        ..
    } = config;

    tracing::info!(
        "Initializing Merkle tree database at `{path}` (max open files: {max_open_files:?}) with {multi_get_chunk_size} multi-get chunk size, \
         {block_cache_capacity}B block cache (indices & filters included: {include_indices_and_filters_in_block_cache:?})",
        path = path.display()
    );

    let mut db = RocksDB::with_options(
        path,
        RocksDBOptions {
            block_cache_capacity: Some(block_cache_capacity),
            include_indices_and_filters_in_block_cache,
            large_memtable_capacity: Some(TreeManagerConfig::MEMTABLE_CAPACITY),
            stalled_writes_retries: StalledWritesRetries::new(
                TreeManagerConfig::STALLED_WRITES_TIMEOUT,
            ),
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

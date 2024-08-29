use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_storage::RocksDB;
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};
use zksync_utils::u256_to_h256;
use zksync_vm_interface::storage::{ReadStorage, StorageSnapshot, StorageWithSnapshot};

use crate::{PostgresStorage, RocksdbStorage, RocksdbStorageBuilder, StateKeeperColumnFamily};

/// Storage with a static lifetime that can be sent to Tokio tasks etc.
pub type OwnedStorage = CommonStorage<'static>;

/// Factory that can produce storage instances on demand. The storage type is encapsulated as a type param
/// (mostly for testing purposes); the default is [`OwnedStorage`].
#[async_trait]
pub trait ReadStorageFactory<S = OwnedStorage>: Debug + Send + Sync + 'static {
    /// Creates a storage instance, e.g. over a Postgres connection or a RocksDB instance.
    /// The specific criteria on which one are left up to the implementation.
    ///
    /// Implementations may be cancel-aware and return `Ok(None)` iff `stop_receiver` receives
    /// a stop signal; this is the only case in which `Ok(None)` should be returned.
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<S>>;
}

/// [`ReadStorageFactory`] producing Postgres-backed storage instances. Hence, it is slower than more advanced
/// alternatives with RocksDB caches and should be used sparingly (e.g., for testing).
#[async_trait]
impl ReadStorageFactory for ConnectionPool<Core> {
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<OwnedStorage>> {
        let connection = self.connection().await?;
        let storage = OwnedStorage::postgres(connection, l1_batch_number).await?;
        Ok(Some(storage.into()))
    }
}

/// DB difference introduced by one batch.
#[derive(Debug, Clone)]
pub struct BatchDiff {
    /// Storage slots touched by this batch along with new values there.
    pub state_diff: HashMap<H256, H256>,
    /// Initial write indices introduced by this batch.
    pub enum_index_diff: HashMap<H256, u64>,
    /// Factory dependencies introduced by this batch.
    pub factory_dep_diff: HashMap<H256, Vec<u8>>,
}

/// A RocksDB cache instance with in-memory DB diffs that gives access to DB state at batches `N` to
/// `N + K`, where `K` is the number of diffs.
#[derive(Debug)]
pub struct RocksdbWithMemory {
    /// RocksDB cache instance caught up to batch `N`.
    pub rocksdb: RocksdbStorage,
    /// Diffs for batches `N + 1` to `N + K`.
    pub batch_diffs: Vec<BatchDiff>,
}

/// Union of all [`ReadStorage`] implementations that are returned by [`ReadStorageFactory`], such as
/// Postgres- and RocksDB-backed storages.
///
/// Ordinarily, you might want to use the [`OwnedStorage`] type alias instead of using `CommonStorage` directly.
/// The former naming signals that the storage has static lifetime and thus can be sent to Tokio tasks or other threads.
#[derive(Debug)]
pub enum CommonStorage<'a> {
    /// Implementation over a Postgres connection.
    Postgres(PostgresStorage<'a>),
    /// Implementation over a RocksDB cache instance.
    Rocksdb(RocksdbStorage),
    /// Implementation over a RocksDB cache instance with in-memory DB diffs.
    RocksdbWithMemory(RocksdbWithMemory),
    /// In-memory storage snapshot with the Postgres storage fallback.
    Snapshot(StorageWithSnapshot<PostgresStorage<'a>>),
    /// Generic implementation. Should be used for testing purposes only since it has performance penalty because
    /// of the dynamic dispatch.
    Boxed(Box<dyn ReadStorage + Send + 'a>),
}

impl<'a> CommonStorage<'a> {
    /// Creates a boxed storage. Should be used for testing purposes only.
    pub fn boxed(storage: impl ReadStorage + Send + 'a) -> Self {
        Self::Boxed(Box::new(storage))
    }
}

impl CommonStorage<'static> {
    /// Creates a Postgres-based storage. Because of the `'static` lifetime requirement, `connection` must be
    /// non-transactional.
    ///
    /// # Errors
    ///
    /// Propagates Postgres I/O errors.
    pub async fn postgres(
        mut connection: Connection<'static, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<PostgresStorage<'static>> {
        let l2_block_number = if let Some((_, l2_block_number)) = connection
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await?
        {
            l2_block_number
        } else {
            tracing::info!("Could not find latest sealed L2 block, loading from snapshot");
            let snapshot_recovery = connection
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await?
                .context("Could not find snapshot, no state available")?;
            if snapshot_recovery.l1_batch_number != l1_batch_number {
                anyhow::bail!(
                    "Snapshot contains L1 batch #{} while #{l1_batch_number} was expected",
                    snapshot_recovery.l1_batch_number
                );
            }
            snapshot_recovery.l2_block_number
        };
        tracing::debug!(%l1_batch_number, %l2_block_number, "Using Postgres-based storage");
        PostgresStorage::new_async(Handle::current(), connection, l2_block_number, true).await
    }

    /// Catches up RocksDB synchronously (i.e. assumes the gap is small) and
    /// returns a [`ReadStorage`] implementation backed by caught-up RocksDB.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn rocksdb(
        connection: &mut Connection<'_, Core>,
        rocksdb: RocksDB<StateKeeperColumnFamily>,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Self>> {
        tracing::debug!("Catching up RocksDB synchronously");
        let rocksdb_builder = RocksdbStorageBuilder::from_rocksdb(rocksdb);
        let rocksdb = rocksdb_builder
            .synchronize(connection, stop_receiver, None)
            .await
            .context("Failed to catch up state keeper RocksDB storage to Postgres")?;
        let Some(rocksdb) = rocksdb else {
            tracing::info!("Synchronizing RocksDB interrupted");
            return Ok(None);
        };
        let rocksdb_l1_batch_number = rocksdb
            .l1_batch_number()
            .await
            .ok_or_else(|| anyhow::anyhow!("No L1 batches available in Postgres"))?;
        if l1_batch_number + 1 != rocksdb_l1_batch_number {
            anyhow::bail!(
                "RocksDB synchronized to L1 batch #{} while #{} was expected",
                rocksdb_l1_batch_number,
                l1_batch_number
            );
        }
        tracing::debug!(%rocksdb_l1_batch_number, "Using RocksDB-based storage");
        Ok(Some(rocksdb.into()))
    }

    /// Creates a storage snapshot. Require protective reads to be persisted for the batch, otherwise
    /// will return `Ok(None)`.
    #[tracing::instrument(skip(connection))]
    pub async fn snapshot(
        connection: &mut Connection<'static, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<StorageSnapshot>> {
        let Some(header) = connection
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
        else {
            return Ok(None);
        };
        let bytecode_hashes: HashSet<_> = header
            .used_contract_hashes
            .into_iter()
            .map(u256_to_h256)
            .collect();

        // Check protective reads early on.
        let protective_reads = connection
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(l1_batch_number)
            .await?;
        if protective_reads.is_empty() {
            tracing::debug!("No protective reads for batch");
            return Ok(None);
        }
        let protective_reads_len = protective_reads.len();
        tracing::debug!("Loaded {protective_reads_len} protective reads");

        let touched_slots = connection
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(l1_batch_number)
            .await?;
        tracing::debug!("Loaded {} touched keys", touched_slots.len());

        let all_accessed_keys: Vec<_> = protective_reads
            .into_iter()
            .map(|key| key.hashed_key())
            .chain(touched_slots.into_keys())
            .collect();
        let previous_values = connection
            .storage_logs_dal()
            .get_previous_storage_values(&all_accessed_keys, l1_batch_number)
            .await?;
        tracing::debug!(
            "Obtained {} previous values for accessed keys",
            previous_values.len()
        );
        let initial_write_info = connection
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&all_accessed_keys)
            .await?;
        tracing::debug!("Obtained initial write info for accessed keys");

        let bytecodes = connection
            .factory_deps_dal()
            .get_factory_deps(&bytecode_hashes)
            .await;
        tracing::debug!("Loaded {} bytecodes used in the batch", bytecodes.len());
        let factory_deps = bytecodes
            .into_iter()
            .map(|(hash_u256, words)| {
                let bytes: Vec<u8> = words.into_iter().flatten().collect();
                (u256_to_h256(hash_u256), bytes)
            })
            .collect();

        let storage = previous_values.into_iter().map(|(key, prev_value)| {
            let prev_value = prev_value.unwrap_or_default();
            let enum_index =
                initial_write_info
                    .get(&key)
                    .copied()
                    .and_then(|(l1_batch, enum_index)| {
                        // Account for enum indexes assigned "in the future"
                        (l1_batch < l1_batch_number).then_some(enum_index)
                    });
            (key, (prev_value, enum_index))
        });
        let storage = storage.collect();
        Ok(Some(StorageSnapshot::new(storage, factory_deps)))
    }
}

impl ReadStorage for RocksdbWithMemory {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let hashed_key = key.hashed_key();
        match self
            .batch_diffs
            .iter()
            .rev()
            .find_map(|b| b.state_diff.get(&hashed_key))
        {
            None => self.rocksdb.read_value(key),
            Some(value) => *value,
        }
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.enum_index_diff.get(&key.hashed_key()))
        {
            None => self.rocksdb.is_write_initial(key),
            Some(_) => false,
        }
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.factory_dep_diff.get(&hash))
        {
            None => self.rocksdb.load_factory_dep(hash),
            Some(value) => Some(value.clone()),
        }
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        match self
            .batch_diffs
            .iter()
            .find_map(|b| b.enum_index_diff.get(&key.hashed_key()))
        {
            None => self.rocksdb.get_enumeration_index(key),
            Some(value) => Some(*value),
        }
    }
}

impl ReadStorage for CommonStorage<'_> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        match self {
            Self::Postgres(postgres) => postgres.read_value(key),
            Self::Rocksdb(rocksdb) => rocksdb.read_value(key),
            Self::RocksdbWithMemory(rocksdb_mem) => rocksdb_mem.read_value(key),
            Self::Snapshot(snapshot) => snapshot.read_value(key),
            Self::Boxed(storage) => storage.read_value(key),
        }
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        match self {
            Self::Postgres(postgres) => postgres.is_write_initial(key),
            Self::Rocksdb(rocksdb) => rocksdb.is_write_initial(key),
            Self::RocksdbWithMemory(rocksdb_mem) => rocksdb_mem.is_write_initial(key),
            Self::Snapshot(snapshot) => snapshot.is_write_initial(key),
            Self::Boxed(storage) => storage.is_write_initial(key),
        }
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        match self {
            Self::Postgres(postgres) => postgres.load_factory_dep(hash),
            Self::Rocksdb(rocksdb) => rocksdb.load_factory_dep(hash),
            Self::RocksdbWithMemory(rocksdb_mem) => rocksdb_mem.load_factory_dep(hash),
            Self::Snapshot(snapshot) => snapshot.load_factory_dep(hash),
            Self::Boxed(storage) => storage.load_factory_dep(hash),
        }
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        match self {
            Self::Postgres(postgres) => postgres.get_enumeration_index(key),
            Self::Rocksdb(rocksdb) => rocksdb.get_enumeration_index(key),
            Self::RocksdbWithMemory(rocksdb_mem) => rocksdb_mem.get_enumeration_index(key),
            Self::Snapshot(snapshot) => snapshot.get_enumeration_index(key),
            Self::Boxed(storage) => storage.get_enumeration_index(key),
        }
    }
}

impl<'a> From<PostgresStorage<'a>> for CommonStorage<'a> {
    fn from(value: PostgresStorage<'a>) -> Self {
        Self::Postgres(value)
    }
}

impl From<RocksdbStorage> for CommonStorage<'_> {
    fn from(value: RocksdbStorage) -> Self {
        Self::Rocksdb(value)
    }
}

impl<'a> From<StorageWithSnapshot<PostgresStorage<'a>>> for CommonStorage<'a> {
    fn from(value: StorageWithSnapshot<PostgresStorage<'a>>) -> Self {
        Self::Snapshot(value)
    }
}

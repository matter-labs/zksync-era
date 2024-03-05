//! RocksDB storage for VM state.
//!
//! ## Storage layout
//!
//! This database has 3 column families:
//!
//! - State
//! - Contracts
//! - Factory dependencies
//!
//! | Column       | Key                             | Value                           | Description                               |
//! | ------------ | ------------------------------- | ------------------------------- | ----------------------------------------- |
//! | State        | 'block_number'                  | serialized block number         | Last processed L1 batch number (u32)      |
//! | State        | 'enum_index_migration_cursor'   | serialized hashed key or empty  | If key is not present it means that the migration hasn't started.                   |
//! |              |                                 | bytes                           | If value is of length 32 then it represents hashed_key migration should start from. |
//! |              |                                 |                                 | If value is empty then it means the migration has finished                          |
//! | State        | hashed `StorageKey`             | 32 bytes value ++ 8 bytes index | State value for the given key             |
//! |              |                                 |                    (big-endian) |                                           |
//! | Contracts    | address (20 bytes)              | `Vec<u8>`                       | Contract contents                         |
//! | Factory deps | hash (32 bytes)                 | `Vec<u8>`                       | Bytecodes for new contracts that a certain contract may deploy. |

use std::{
    collections::HashMap,
    convert::TryInto,
    mem,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Context as _;
use itertools::{Either, Itertools};
use tokio::sync::watch;
use zksync_dal::StorageProcessor;
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256, U256};
use zksync_utils::{h256_to_u256, u256_to_h256};

use self::metrics::METRICS;
#[cfg(test)]
use self::tests::RocksdbStorageEventListener;
use crate::{InMemoryStorage, ReadStorage};

mod metrics;
mod recovery;
#[cfg(test)]
mod tests;

fn serialize_l1_batch_number(block_number: u32) -> [u8; 4] {
    block_number.to_le_bytes()
}

fn deserialize_l1_batch_number(bytes: &[u8]) -> u32 {
    let bytes: [u8; 4] = bytes.try_into().expect("incorrect block number format");
    u32::from_le_bytes(bytes)
}

#[derive(Debug, Clone, Copy)]
enum StateKeeperColumnFamily {
    State,
    Contracts,
    FactoryDeps,
}

impl NamedColumnFamily for StateKeeperColumnFamily {
    const DB_NAME: &'static str = "state_keeper";
    const ALL: &'static [Self] = &[Self::State, Self::Contracts, Self::FactoryDeps];

    fn name(&self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Contracts => "contracts",
            Self::FactoryDeps => "factory_deps",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StateValue {
    pub value: H256,
    pub enum_index: Option<u64>,
}

impl StateValue {
    pub fn new(value: H256, enum_index: Option<u64>) -> Self {
        Self { value, enum_index }
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        if bytes.len() == 32 {
            Self {
                value: H256::from_slice(bytes),
                enum_index: None,
            }
        } else {
            Self {
                value: H256::from_slice(&bytes[..32]),
                enum_index: Some(u64::from_be_bytes(bytes[32..40].try_into().unwrap())),
            }
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(40);
        buffer.extend_from_slice(self.value.as_bytes());
        if let Some(index) = self.enum_index {
            buffer.extend_from_slice(&index.to_be_bytes());
        }
        buffer
    }
}

/// Error emitted when [`RocksdbStorage`] is being updated.
#[derive(Debug)]
enum RocksdbSyncError {
    Internal(anyhow::Error),
    Interrupted,
}

impl From<anyhow::Error> for RocksdbSyncError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

/// [`ReadStorage`] implementation backed by RocksDB.
#[derive(Debug)]
pub struct RocksdbStorage {
    db: RocksDB<StateKeeperColumnFamily>,
    pending_patch: InMemoryStorage,
    enum_index_migration_chunk_size: usize,
    /// Test-only listeners to events produced by the storage.
    #[cfg(test)]
    listener: RocksdbStorageEventListener,
}

/// Builder of [`RocksdbStorage`]. The storage data is inaccessible until the storage is [`Self::synchronize()`]d
/// with Postgres.
#[derive(Debug)]
pub struct RocksbStorageBuilder(RocksdbStorage);

impl RocksbStorageBuilder {
    /// Enables enum indices migration.
    pub fn enable_enum_index_migration(&mut self, chunk_size: usize) {
        self.0.enum_index_migration_chunk_size = chunk_size;
    }

    /// Returns the last processed l1 batch number + 1.
    ///
    /// # Panics
    ///
    /// Panics on RocksDB errors.
    pub async fn l1_batch_number(&self) -> Option<L1BatchNumber> {
        self.0.l1_batch_number().await
    }

    /// Synchronizes this storage with Postgres using the provided connection.
    ///
    /// # Return value
    ///
    /// Returns `Ok(None)` if the update is interrupted using `stop_receiver`.
    ///
    /// # Errors
    ///
    /// - Errors if the local L1 batch number is greater than the last sealed L1 batch number
    ///   in Postgres.
    pub async fn synchronize(
        self,
        storage: &mut StorageProcessor<'_>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<RocksdbStorage>> {
        let mut inner = self.0;
        match inner.update_from_postgres(storage, stop_receiver).await {
            Ok(()) => Ok(Some(inner)),
            Err(RocksdbSyncError::Interrupted) => Ok(None),
            Err(RocksdbSyncError::Internal(err)) => Err(err),
        }
    }

    /// Rolls back the state to a previous L1 batch number.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn rollback(
        mut self,
        storage: &mut StorageProcessor<'_>,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        self.0.rollback(storage, last_l1_batch_to_keep).await
    }
}

impl RocksdbStorage {
    const L1_BATCH_NUMBER_KEY: &'static [u8] = b"block_number";
    const ENUM_INDEX_MIGRATION_CURSOR: &'static [u8] = b"enum_index_migration_cursor";

    /// Desired size of log chunks loaded from Postgres during snapshot recovery.
    /// This is intentionally not configurable because chunks must be the same for the entire recovery
    /// (i.e., not changed after a node restart).
    const DESIRED_LOG_CHUNK_SIZE: u64 = 200_000;

    fn is_special_key(key: &[u8]) -> bool {
        key == Self::L1_BATCH_NUMBER_KEY || key == Self::ENUM_INDEX_MIGRATION_CURSOR
    }

    /// Creates a new storage builder with the provided RocksDB `path`.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB I/O errors.
    pub async fn builder(path: &Path) -> anyhow::Result<RocksbStorageBuilder> {
        Self::new(path.to_path_buf())
            .await
            .map(RocksbStorageBuilder)
    }

    async fn new(path: PathBuf) -> anyhow::Result<Self> {
        tokio::task::spawn_blocking(move || {
            Ok(Self {
                db: RocksDB::new(&path).context("failed initializing state keeper RocksDB")?,
                pending_patch: InMemoryStorage::default(),
                enum_index_migration_chunk_size: 100,
                #[cfg(test)]
                listener: RocksdbStorageEventListener::default(),
            })
        })
        .await
        .context("panicked initializing state keeper RocksDB")?
    }

    async fn update_from_postgres(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Result<(), RocksdbSyncError> {
        let mut current_l1_batch_number = self
            .ensure_ready(storage, Self::DESIRED_LOG_CHUNK_SIZE, stop_receiver)
            .await?;

        let latency = METRICS.update.start();
        let Some(latest_l1_batch_number) = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("failed fetching sealed L1 batch number")?
        else {
            // No L1 batches are persisted in Postgres; update is not necessary.
            return Ok(());
        };
        tracing::debug!("Loading storage for l1 batch number {latest_l1_batch_number}");

        if current_l1_batch_number > latest_l1_batch_number + 1 {
            let err = anyhow::anyhow!(
                "L1 batch number in state keeper cache ({current_l1_batch_number}) is greater than \
                 the last sealed L1 batch number in Postgres ({latest_l1_batch_number})"
            );
            return Err(err.into());
        }

        while current_l1_batch_number <= latest_l1_batch_number {
            if *stop_receiver.borrow() {
                return Err(RocksdbSyncError::Interrupted);
            }
            let current_lag = latest_l1_batch_number.0 - current_l1_batch_number.0 + 1;
            METRICS.lag.set(current_lag.into());

            tracing::debug!("Loading state changes for l1 batch {current_l1_batch_number}");
            let storage_logs = storage
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(current_l1_batch_number)
                .await
                .with_context(|| {
                    format!("failed loading touched slots for L1 batch {current_l1_batch_number}")
                })?;
            self.apply_storage_logs(storage_logs, storage).await?;

            tracing::debug!("Loading factory deps for L1 batch {current_l1_batch_number}");
            let factory_deps = storage
                .blocks_dal()
                .get_l1_batch_factory_deps(current_l1_batch_number)
                .await
                .with_context(|| {
                    format!("failed loading factory deps for L1 batch {current_l1_batch_number}")
                })?;
            for (hash, bytecode) in factory_deps {
                self.store_factory_dep(hash, bytecode);
            }

            current_l1_batch_number += 1;
            self.save(Some(current_l1_batch_number))
                .await
                .with_context(|| format!("failed saving L1 batch #{current_l1_batch_number}"))?;
            #[cfg(test)]
            (self.listener.on_l1_batch_synced)(current_l1_batch_number - 1);
        }

        latency.observe();
        METRICS.lag.set(0);
        let estimated_size = self.estimated_map_size();
        METRICS.size.set(estimated_size);
        tracing::info!(
            "Secondary storage for L1 batch #{latest_l1_batch_number} initialized, size is {estimated_size}"
        );

        assert!(self.enum_index_migration_chunk_size > 0);
        // Enum indices must be at the storage. Run migration till the end.
        while self.enum_migration_start_from().await.is_some() {
            if *stop_receiver.borrow() {
                return Err(RocksdbSyncError::Interrupted);
            }
            self.save_missing_enum_indices(storage).await?;
        }
        Ok(())
    }

    async fn apply_storage_logs(
        &mut self,
        storage_logs: HashMap<StorageKey, H256>,
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<()> {
        let db = self.db.clone();
        let processed_logs =
            tokio::task::spawn_blocking(move || Self::process_transaction_logs(&db, storage_logs))
                .await
                .context("panicked processing storage logs")?;

        let (logs_with_known_indices, logs_with_unknown_indices): (Vec<_>, Vec<_>) = processed_logs
            .into_iter()
            .partition_map(|(key, StateValue { value, enum_index })| match enum_index {
                Some(index) => Either::Left((key.hashed_key(), (value, index))),
                None => Either::Right((key.hashed_key(), value)),
            });
        let keys_with_unknown_indices: Vec<_> = logs_with_unknown_indices
            .iter()
            .map(|&(key, _)| key)
            .collect();

        let enum_indices_and_batches = storage
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&keys_with_unknown_indices)
            .await
            .context("failed getting enumeration indices for storage logs")?;
        anyhow::ensure!(
            keys_with_unknown_indices.len() == enum_indices_and_batches.len(),
            "Inconsistent Postgres data: not all new storage logs have enumeration indices"
        );
        self.pending_patch.state = logs_with_known_indices
            .into_iter()
            .chain(
                logs_with_unknown_indices
                    .into_iter()
                    .map(|(key, value)| (key, (value, enum_indices_and_batches[&key].1))),
            )
            .collect();
        Ok(())
    }

    async fn save_missing_enum_indices(
        &self,
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<()> {
        let (true, Some(start_from)) = (
            self.enum_index_migration_chunk_size > 0,
            self.enum_migration_start_from().await,
        ) else {
            return Ok(());
        };

        let started_at = Instant::now();
        tracing::info!(
            "RocksDB enum index migration is not finished, starting from key {start_from:0>64x}"
        );

        let db = self.db.clone();
        let enum_index_migration_chunk_size = self.enum_index_migration_chunk_size;
        let (keys, values): (Vec<_>, Vec<_>) = tokio::task::spawn_blocking(move || {
            db.from_iterator_cf(StateKeeperColumnFamily::State, start_from.as_bytes())
                .filter_map(|(key, value)| {
                    if Self::is_special_key(&key) {
                        return None;
                    }
                    let state_value = StateValue::deserialize(&value);
                    state_value
                        .enum_index
                        .is_none()
                        .then(|| (H256::from_slice(&key), state_value.value))
                })
                .take(enum_index_migration_chunk_size)
                .unzip()
        })
        .await
        .unwrap();

        let enum_indices_and_batches = storage
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&keys)
            .await
            .context("failed getting enumeration indices for storage logs")?;
        assert_eq!(keys.len(), enum_indices_and_batches.len());
        let key_count = keys.len();

        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let mut write_batch = db.new_write_batch();
            for (key, value) in keys.iter().zip(values) {
                let index = enum_indices_and_batches[key].1;
                write_batch.put_cf(
                    StateKeeperColumnFamily::State,
                    key.as_bytes(),
                    &StateValue::new(value, Some(index)).serialize(),
                );
            }

            let next_key = keys
                .last()
                .and_then(|last_key| h256_to_u256(*last_key).checked_add(U256::one()))
                .map(u256_to_h256);
            match (next_key, keys.len()) {
                (Some(next_key), keys_len) if keys_len == enum_index_migration_chunk_size => {
                    write_batch.put_cf(
                        StateKeeperColumnFamily::State,
                        Self::ENUM_INDEX_MIGRATION_CURSOR,
                        next_key.as_bytes(),
                    );
                }
                _ => {
                    write_batch.put_cf(
                        StateKeeperColumnFamily::State,
                        Self::ENUM_INDEX_MIGRATION_CURSOR,
                        &[],
                    );
                    tracing::info!("RocksDB enum index migration finished");
                }
            }
            db.write(write_batch)
                .context("failed saving enum indices to RocksDB")
        })
        .await
        .context("panicked while saving enum indices to RocksDB")??;

        tracing::info!(
            "RocksDB enum index migration chunk took {:?}, migrated {key_count} keys",
            started_at.elapsed()
        );
        Ok(())
    }

    fn read_value_inner(&self, key: &StorageKey) -> Option<StorageValue> {
        Self::read_state_value(&self.db, key.hashed_key()).map(|state_value| state_value.value)
    }

    fn read_state_value(
        db: &RocksDB<StateKeeperColumnFamily>,
        hashed_key: H256,
    ) -> Option<StateValue> {
        let cf = StateKeeperColumnFamily::State;
        db.get_cf(cf, &Self::serialize_state_key(hashed_key))
            .expect("failed to read rocksdb state value")
            .map(|value| StateValue::deserialize(&value))
    }

    /// Returns storage logs to apply.
    fn process_transaction_logs(
        db: &RocksDB<StateKeeperColumnFamily>,
        updates: HashMap<StorageKey, H256>,
    ) -> Vec<(StorageKey, StateValue)> {
        let it = updates.into_iter().filter_map(move |(key, new_value)| {
            if let Some(state_value) = Self::read_state_value(db, key.hashed_key()) {
                Some((key, StateValue::new(new_value, state_value.enum_index)))
            } else {
                (!new_value.is_zero()).then_some((key, StateValue::new(new_value, None)))
            }
        });
        it.collect()
    }

    /// Stores a factory dependency with the specified `hash` and `bytecode`.
    fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.pending_patch.factory_deps.insert(hash, bytecode);
    }

    async fn rollback(
        &mut self,
        connection: &mut StorageProcessor<'_>,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!("Rolling back state keeper storage to L1 batch #{last_l1_batch_to_keep}...");

        tracing::info!("Getting logs that should be applied to rollback state...");
        let stage_start = Instant::now();
        let logs = connection
            .storage_logs_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep)
            .await
            .context("failed getting logs for rollback")?;
        tracing::info!("Got {} logs, took {:?}", logs.len(), stage_start.elapsed());

        tracing::info!("Getting number of last miniblock for L1 batch #{last_l1_batch_to_keep}...");
        let stage_start = Instant::now();
        let (_, last_miniblock_to_keep) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .await
            .with_context(|| {
                format!("failed fetching miniblock range for L1 batch #{last_l1_batch_to_keep}")
            })?
            .context("L1 batch should contain at least one miniblock")?;
        tracing::info!(
            "Got miniblock number {last_miniblock_to_keep}, took {:?}",
            stage_start.elapsed()
        );

        tracing::info!("Getting factory deps that need to be removed...");
        let stage_start = Instant::now();
        let factory_deps = connection
            .factory_deps_dal()
            .get_factory_deps_for_revert(last_miniblock_to_keep)
            .await
            .with_context(|| {
                format!("failed fetching factory deps for miniblock #{last_miniblock_to_keep}")
            })?;
        tracing::info!(
            "Got {} factory deps, took {:?}",
            factory_deps.len(),
            stage_start.elapsed()
        );

        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let mut batch = db.new_write_batch();

            let cf = StateKeeperColumnFamily::State;
            for (key, maybe_value) in logs {
                if let Some((prev_value, prev_index)) = maybe_value {
                    batch.put_cf(
                        cf,
                        key.as_bytes(),
                        &StateValue::new(prev_value, Some(prev_index)).serialize(),
                    );
                } else {
                    batch.delete_cf(cf, key.as_bytes());
                }
            }
            batch.put_cf(
                cf,
                Self::L1_BATCH_NUMBER_KEY,
                &serialize_l1_batch_number(last_l1_batch_to_keep.0 + 1),
            );

            let cf = StateKeeperColumnFamily::FactoryDeps;
            for factory_dep_hash in &factory_deps {
                batch.delete_cf(cf, factory_dep_hash.as_bytes());
            }

            db.write(batch)
                .context("failed to save state data into RocksDB")
        })
        .await
        .context("panicked during revert")?
    }

    /// Saves the pending changes to RocksDB. Must be executed on a Tokio thread.
    async fn save(&mut self, l1_batch_number: Option<L1BatchNumber>) -> anyhow::Result<()> {
        let pending_patch = mem::take(&mut self.pending_patch);

        let db = self.db.clone();
        let save_task = tokio::task::spawn_blocking(move || {
            let mut batch = db.new_write_batch();
            let cf = StateKeeperColumnFamily::State;
            if let Some(l1_batch_number) = l1_batch_number {
                batch.put_cf(
                    cf,
                    Self::L1_BATCH_NUMBER_KEY,
                    &serialize_l1_batch_number(l1_batch_number.0),
                );
            }
            for (key, (value, enum_index)) in pending_patch.state {
                batch.put_cf(
                    cf,
                    &Self::serialize_state_key(key),
                    &StateValue::new(value, Some(enum_index)).serialize(),
                );
            }

            let cf = StateKeeperColumnFamily::FactoryDeps;
            for (hash, value) in pending_patch.factory_deps {
                batch.put_cf(cf, &hash.to_fixed_bytes(), value.as_ref());
            }
            db.write(batch)
                .context("failed to save state data into RocksDB")
        });
        save_task
            .await
            .context("panicked when saving state data into RocksDB")?
    }

    /// Returns the last processed l1 batch number + 1.
    ///
    /// # Panics
    ///
    /// Panics on RocksDB errors.
    async fn l1_batch_number(&self) -> Option<L1BatchNumber> {
        let cf = StateKeeperColumnFamily::State;
        let db = self.db.clone();
        let number_bytes =
            tokio::task::spawn_blocking(move || db.get_cf(cf, Self::L1_BATCH_NUMBER_KEY))
                .await
                .expect("failed getting L1 batch number from RocksDB")
                .expect("failed getting L1 batch number from RocksDB");
        number_bytes.map(|bytes| L1BatchNumber(deserialize_l1_batch_number(&bytes)))
    }

    fn serialize_state_key(key: H256) -> [u8; 32] {
        key.to_fixed_bytes()
    }

    /// Estimates the number of key–value entries in the VM state.
    fn estimated_map_size(&self) -> u64 {
        self.db
            .estimated_number_of_entries(StateKeeperColumnFamily::State)
    }

    async fn enum_migration_start_from(&self) -> Option<H256> {
        let db = self.db.clone();
        let value = tokio::task::spawn_blocking(move || {
            db.get_cf(
                StateKeeperColumnFamily::State,
                Self::ENUM_INDEX_MIGRATION_CURSOR,
            )
            .expect("failed to read `ENUM_INDEX_MIGRATION_CURSOR`")
        })
        .await
        .unwrap();

        match value {
            Some(value) if value.is_empty() => None,
            Some(cursor) => Some(H256::from_slice(&cursor)),
            None => Some(H256::zero()),
        }
    }
}

impl ReadStorage for RocksdbStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.read_value_inner(key).unwrap_or_else(H256::zero)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.read_value_inner(key).is_none()
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let cf = StateKeeperColumnFamily::FactoryDeps;
        self.db
            .get_cf(cf, hash.as_bytes())
            .expect("failed to read RocksDB state value")
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        // Can safely unwrap here since it indicates that the migration has not yet ended and boojum will
        // only be deployed when the migration is finished.
        Self::read_state_value(&self.db, key.hashed_key())
            .map(|state_value| state_value.enum_index.unwrap())
    }
}

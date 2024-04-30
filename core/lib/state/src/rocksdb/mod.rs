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
//! | State        | 'enum_index_migration_cursor'   | serialized hashed key or empty  | Deprecated                                |
//! |              |                                 | bytes                           |                                           |
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
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};

#[cfg(test)]
use self::tests::RocksdbStorageEventListener;
use self::{metrics::METRICS, recovery::Strategy};
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

/// RocksDB column families used by the state keeper.
#[derive(Debug, Clone, Copy)]
pub enum StateKeeperColumnFamily {
    /// Column family containing tree state information.
    State,
    /// Column family containing contract contents.
    Contracts,
    /// Column family containing bytecodes for new contracts that a certain contract may deploy.
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
#[derive(Debug, Clone)]
pub struct RocksdbStorage {
    db: RocksDB<StateKeeperColumnFamily>,
    pending_patch: InMemoryStorage,
    /// Test-only listeners to events produced by the storage.
    #[cfg(test)]
    listener: RocksdbStorageEventListener,
}

/// Builder of [`RocksdbStorage`]. The storage data is inaccessible until the storage is [`Self::synchronize()`]d
/// with Postgres.
#[derive(Debug)]
pub struct RocksdbStorageBuilder(RocksdbStorage);

impl RocksdbStorageBuilder {
    /// Create a builder from an existing RocksDB instance.
    pub fn from_rocksdb(value: RocksDB<StateKeeperColumnFamily>) -> Self {
        RocksdbStorageBuilder(RocksdbStorage {
            db: value,
            pending_patch: InMemoryStorage::default(),
            #[cfg(test)]
            listener: RocksdbStorageEventListener::default(),
        })
    }

    /// Returns the last processed l1 batch number + 1.
    ///
    /// # Panics
    ///
    /// Panics on RocksDB errors.
    pub async fn l1_batch_number(&self) -> Option<L1BatchNumber> {
        self.0.l1_batch_number().await
    }

    /// Ensures that the storage is ready to process L1 batches (i.e., has completed snapshot recovery).
    ///
    /// # Return value
    ///
    /// Returns a flag indicating whether snapshot recovery was performed.
    ///
    /// # Errors
    ///
    /// Returns I/O RocksDB and Postgres errors.
    pub async fn ensure_ready(
        &mut self,
        storage: &mut Connection<'_, Core>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<bool> {
        let ready_result = self
            .0
            .ensure_ready(
                storage,
                RocksdbStorage::DESIRED_LOG_CHUNK_SIZE,
                stop_receiver,
            )
            .await;
        match ready_result {
            Ok((strategy, _)) => Ok(matches!(strategy, Strategy::Recovery)),
            Err(RocksdbSyncError::Interrupted) => Ok(false),
            Err(RocksdbSyncError::Internal(err)) => Err(err),
        }
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
        storage: &mut Connection<'_, Core>,
        stop_receiver: &watch::Receiver<bool>,
        to_l1_batch_number: Option<L1BatchNumber>,
    ) -> anyhow::Result<Option<RocksdbStorage>> {
        let mut inner = self.0;
        match inner
            .update_from_postgres(storage, stop_receiver, to_l1_batch_number)
            .await
        {
            Ok(()) => Ok(Some(inner)),
            Err(RocksdbSyncError::Interrupted) => Ok(None),
            Err(RocksdbSyncError::Internal(err)) => Err(err),
        }
    }

    /// Reverts the state to a previous L1 batch number.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn roll_back(
        mut self,
        storage: &mut Connection<'_, Core>,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        self.0.revert(storage, last_l1_batch_to_keep).await
    }
}

impl RocksdbStorage {
    const L1_BATCH_NUMBER_KEY: &'static [u8] = b"block_number";
    #[allow(dead_code)]
    const ENUM_INDEX_MIGRATION_CURSOR: &'static [u8] = b"enum_index_migration_cursor";

    /// Desired size of log chunks loaded from Postgres during snapshot recovery.
    /// This is intentionally not configurable because chunks must be the same for the entire recovery
    /// (i.e., not changed after a node restart).
    const DESIRED_LOG_CHUNK_SIZE: u64 = 200_000;

    #[allow(dead_code)]
    fn is_special_key(key: &[u8]) -> bool {
        key == Self::L1_BATCH_NUMBER_KEY || key == Self::ENUM_INDEX_MIGRATION_CURSOR
    }

    /// Creates a new storage builder with the provided RocksDB `path`.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB I/O errors.
    pub async fn builder(path: &Path) -> anyhow::Result<RocksdbStorageBuilder> {
        Self::new(path.to_path_buf())
            .await
            .map(RocksdbStorageBuilder)
    }

    async fn new(path: PathBuf) -> anyhow::Result<Self> {
        tokio::task::spawn_blocking(move || {
            Ok(Self {
                db: RocksDB::new(&path).context("failed initializing state keeper RocksDB")?,
                pending_patch: InMemoryStorage::default(),
                #[cfg(test)]
                listener: RocksdbStorageEventListener::default(),
            })
        })
        .await
        .context("panicked initializing state keeper RocksDB")?
    }

    async fn update_from_postgres(
        &mut self,
        storage: &mut Connection<'_, Core>,
        stop_receiver: &watch::Receiver<bool>,
        to_l1_batch_number: Option<L1BatchNumber>,
    ) -> Result<(), RocksdbSyncError> {
        let (_, mut current_l1_batch_number) = self
            .ensure_ready(storage, Self::DESIRED_LOG_CHUNK_SIZE, stop_receiver)
            .await?;

        let latency = METRICS.update.start();
        let Some(latest_l1_batch_number) = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .map_err(DalError::generalize)?
        else {
            // No L1 batches are persisted in Postgres; update is not necessary.
            return Ok(());
        };
        let to_l1_batch_number = if let Some(to_l1_batch_number) = to_l1_batch_number {
            if to_l1_batch_number > latest_l1_batch_number {
                let err = anyhow::anyhow!(
                    "Requested to update RocksDB to L1 batch number ({current_l1_batch_number}) that \
                     is greater than the last sealed L1 batch number in Postgres ({latest_l1_batch_number})"
                );
                return Err(err.into());
            }
            to_l1_batch_number
        } else {
            latest_l1_batch_number
        };
        tracing::debug!("Loading storage for l1 batch number {to_l1_batch_number}");

        if current_l1_batch_number > to_l1_batch_number + 1 {
            let err = anyhow::anyhow!(
                "L1 batch number in state keeper cache ({current_l1_batch_number}) is greater than \
                 the requested batch number ({to_l1_batch_number})"
            );
            return Err(err.into());
        }

        while current_l1_batch_number <= to_l1_batch_number {
            if *stop_receiver.borrow() {
                return Err(RocksdbSyncError::Interrupted);
            }
            let current_lag = to_l1_batch_number.0 - current_l1_batch_number.0 + 1;
            METRICS.lag.set(current_lag.into());

            tracing::debug!("Loading state changes for l1 batch {current_l1_batch_number}");
            let storage_logs = storage
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(current_l1_batch_number)
                .await
                .map_err(DalError::generalize)?;
            self.apply_storage_logs(storage_logs, storage).await?;

            tracing::debug!("Loading factory deps for L1 batch {current_l1_batch_number}");
            let factory_deps = storage
                .blocks_dal()
                .get_l1_batch_factory_deps(current_l1_batch_number)
                .await
                .map_err(DalError::generalize)?;
            for (hash, bytecode) in factory_deps {
                self.store_factory_dep(hash, bytecode);
            }

            current_l1_batch_number += 1;
            self.save(Some(current_l1_batch_number))
                .await
                .with_context(|| format!("failed saving L1 batch #{current_l1_batch_number}"))?;
            #[cfg(test)]
            (self.listener.on_l1_batch_synced.write().await)(current_l1_batch_number - 1);
        }

        latency.observe();
        METRICS.lag.set(0);
        let estimated_size = self.estimated_map_size();
        METRICS.size.set(estimated_size);
        tracing::info!(
            "Secondary storage for L1 batch #{to_l1_batch_number} initialized, size is {estimated_size}"
        );

        Ok(())
    }

    async fn apply_storage_logs(
        &mut self,
        storage_logs: HashMap<StorageKey, H256>,
        storage: &mut Connection<'_, Core>,
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

    async fn revert(
        &mut self,
        connection: &mut Connection<'_, Core>,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!("Reverting state keeper storage to L1 batch #{last_l1_batch_to_keep}...");

        tracing::info!("Getting logs that should be applied to revert the state...");
        let stage_start = Instant::now();
        let logs = connection
            .storage_logs_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep)
            .await
            .context("failed getting logs for revert")?;
        tracing::info!("Got {} logs, took {:?}", logs.len(), stage_start.elapsed());

        tracing::info!("Getting number of last L2 block for L1 batch #{last_l1_batch_to_keep}...");
        let stage_start = Instant::now();
        let (_, last_l2_block_to_keep) = connection
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(last_l1_batch_to_keep)
            .await?
            .context("L1 batch should contain at least one L2 block")?;
        tracing::info!(
            "Got L2 block number {last_l2_block_to_keep}, took {:?}",
            stage_start.elapsed()
        );

        tracing::info!("Getting factory deps that need to be removed...");
        let stage_start = Instant::now();
        let factory_deps = connection
            .factory_deps_dal()
            .get_factory_deps_for_revert(last_l2_block_to_keep)
            .await?;
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
    pub async fn l1_batch_number(&self) -> Option<L1BatchNumber> {
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

    /// Converts self into the underlying RocksDB primitive
    pub fn into_rocksdb(self) -> RocksDB<StateKeeperColumnFamily> {
        self.db
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

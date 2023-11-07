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

use itertools::{Either, Itertools};
use std::{collections::HashMap, convert::TryInto, mem, path::Path, time::Instant};

use zksync_dal::StorageProcessor;
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256, U256};
use zksync_utils::{h256_to_u256, u256_to_h256};

mod metrics;

use self::metrics::METRICS;
use crate::{InMemoryStorage, ReadStorage};

fn serialize_block_number(block_number: u32) -> [u8; 4] {
    block_number.to_le_bytes()
}

fn deserialize_block_number(bytes: &[u8]) -> u32 {
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

/// [`ReadStorage`] implementation backed by RocksDB.
#[derive(Debug)]
pub struct RocksdbStorage {
    db: RocksDB<StateKeeperColumnFamily>,
    pending_patch: InMemoryStorage,
    enum_index_migration_chunk_size: usize,
}

impl RocksdbStorage {
    const BLOCK_NUMBER_KEY: &'static [u8] = b"block_number";
    const ENUM_INDEX_MIGRATION_CURSOR: &'static [u8] = b"enum_index_migration_cursor";

    fn is_special_key(key: &[u8]) -> bool {
        key == Self::BLOCK_NUMBER_KEY || key == Self::ENUM_INDEX_MIGRATION_CURSOR
    }

    /// Creates a new storage with the provided RocksDB `path`.
    pub fn new(path: &Path) -> Self {
        let db = RocksDB::new(path);
        Self {
            db,
            pending_patch: InMemoryStorage::default(),
            enum_index_migration_chunk_size: 100,
        }
    }

    /// Enables enum indices migration.
    pub fn enable_enum_index_migration(&mut self, chunk_size: usize) {
        self.enum_index_migration_chunk_size = chunk_size;
    }

    /// Synchronizes this storage with Postgres using the provided connection.
    ///
    /// # Panics
    ///
    /// Panics if the local L1 batch number is greater than the last sealed L1 batch number
    /// in Postgres.
    pub async fn update_from_postgres(&mut self, conn: &mut StorageProcessor<'_>) {
        let latency = METRICS.update.start();
        let latest_l1_batch_number = conn
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        tracing::debug!(
            "loading storage for l1 batch number {}",
            latest_l1_batch_number.0
        );

        let mut current_l1_batch_number = self.l1_batch_number().0;
        assert!(
            current_l1_batch_number <= latest_l1_batch_number.0 + 1,
            "L1 batch number in state keeper cache ({current_l1_batch_number}) is greater than \
             the last sealed L1 batch number in Postgres ({latest_l1_batch_number})"
        );

        while current_l1_batch_number <= latest_l1_batch_number.0 {
            let current_lag = latest_l1_batch_number.0 - current_l1_batch_number + 1;
            METRICS.lag.set(current_lag.into());

            tracing::debug!("loading state changes for l1 batch {current_l1_batch_number}");
            let storage_logs = conn
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(L1BatchNumber(current_l1_batch_number))
                .await;
            self.apply_storage_logs(storage_logs, conn).await;

            tracing::debug!("loading factory deps for l1 batch {current_l1_batch_number}");
            let factory_deps = conn
                .blocks_dal()
                .get_l1_batch_factory_deps(L1BatchNumber(current_l1_batch_number))
                .await
                .unwrap();
            for (hash, bytecode) in factory_deps {
                self.store_factory_dep(hash, bytecode);
            }

            current_l1_batch_number += 1;
            self.save(L1BatchNumber(current_l1_batch_number)).await;
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
        while self.enum_migration_start_from().is_some() {
            self.save_missing_enum_indices(conn).await;
        }
    }

    async fn apply_storage_logs(
        &mut self,
        storage_logs: HashMap<StorageKey, H256>,
        conn: &mut StorageProcessor<'_>,
    ) {
        let (logs_with_known_indices, logs_with_unknown_indices): (Vec<_>, Vec<_>) = self
            .process_transaction_logs(storage_logs)
            .partition_map(|(key, StateValue { value, enum_index })| match enum_index {
                Some(index) => Either::Left((key, (value, index))),
                None => Either::Right((key, value)),
            });
        let keys_with_unknown_indices: Vec<_> = logs_with_unknown_indices
            .iter()
            .map(|(key, _)| key.hashed_key())
            .collect();

        let enum_indices_and_batches = conn
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&keys_with_unknown_indices)
            .await;
        assert_eq!(
            keys_with_unknown_indices.len(),
            enum_indices_and_batches.len()
        );
        self.pending_patch.state =
            logs_with_known_indices
                .into_iter()
                .chain(logs_with_unknown_indices.into_iter().map(|(key, value)| {
                    (key, (value, enum_indices_and_batches[&key.hashed_key()].1))
                }))
                .collect();
    }

    async fn save_missing_enum_indices(&self, conn: &mut StorageProcessor<'_>) {
        let (Some(start_from), true) = (
            self.enum_migration_start_from(),
            self.enum_index_migration_chunk_size > 0,
        ) else {
            return;
        };

        let started_at = Instant::now();
        tracing::info!(
            "RocksDB enum index migration is not finished, starting from key {start_from:0>64x}"
        );

        let mut write_batch = self.db.new_write_batch();
        let (keys, values): (Vec<_>, Vec<_>) = self
            .db
            .from_iterator_cf(StateKeeperColumnFamily::State, start_from.as_bytes())
            .filter_map(|(key, value)| {
                if Self::is_special_key(&key) {
                    return None;
                }
                let state_value = StateValue::deserialize(&value);
                (state_value.enum_index.is_none())
                    .then(|| (H256::from_slice(&key), state_value.value))
            })
            .take(self.enum_index_migration_chunk_size)
            .unzip();
        let enum_indices_and_batches = conn
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&keys)
            .await;
        assert_eq!(keys.len(), enum_indices_and_batches.len());

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
            (Some(next_key), keys_len) if keys_len == self.enum_index_migration_chunk_size => {
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
        self.db
            .write(write_batch)
            .expect("failed to save state data into rocksdb");
        tracing::info!(
            "RocksDB enum index migration chunk took {:?}, migrated {} keys",
            started_at.elapsed(),
            keys.len()
        );
    }

    fn read_value_inner(&self, key: &StorageKey) -> Option<StorageValue> {
        self.read_state_value(key)
            .map(|state_value| state_value.value)
    }

    fn read_state_value(&self, key: &StorageKey) -> Option<StateValue> {
        let cf = StateKeeperColumnFamily::State;
        self.db
            .get_cf(cf, &Self::serialize_state_key(key))
            .expect("failed to read rocksdb state value")
            .map(|value| StateValue::deserialize(&value))
    }

    /// Returns storage logs to apply.
    fn process_transaction_logs(
        &self,
        updates: HashMap<StorageKey, H256>,
    ) -> impl Iterator<Item = (StorageKey, StateValue)> + '_ {
        updates.into_iter().filter_map(|(key, new_value)| {
            if let Some(state_value) = self.read_state_value(&key) {
                Some((key, StateValue::new(new_value, state_value.enum_index)))
            } else {
                (!new_value.is_zero()).then_some((key, StateValue::new(new_value, None)))
            }
        })
    }

    /// Stores a factory dependency with the specified `hash` and `bytecode`.
    fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.pending_patch.factory_deps.insert(hash, bytecode);
    }

    /// Rolls back the state to a previous L1 batch number.
    ///
    /// # Panics
    ///
    /// Panics on RocksDB errors.
    pub async fn rollback(
        &mut self,
        connection: &mut StorageProcessor<'_>,
        last_l1_batch_to_keep: L1BatchNumber,
    ) {
        tracing::info!("Rolling back state keeper storage to L1 batch #{last_l1_batch_to_keep}...");

        tracing::info!("Getting logs that should be applied to rollback state...");
        let stage_start = Instant::now();
        let logs = connection
            .storage_logs_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep)
            .await;
        tracing::info!("Got {} logs, took {:?}", logs.len(), stage_start.elapsed());

        tracing::info!("Getting number of last miniblock for L1 batch #{last_l1_batch_to_keep}...");
        let stage_start = Instant::now();
        let (_, last_miniblock_to_keep) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .await
            .unwrap()
            .expect("L1 batch should contain at least one miniblock");
        tracing::info!(
            "Got miniblock number {last_miniblock_to_keep}, took {:?}",
            stage_start.elapsed()
        );

        tracing::info!("Getting factory deps that need to be removed...");
        let stage_start = Instant::now();
        let factory_deps = connection
            .storage_dal()
            .get_factory_deps_for_revert(last_miniblock_to_keep)
            .await;
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
                Self::BLOCK_NUMBER_KEY,
                &serialize_block_number(last_l1_batch_to_keep.0 + 1),
            );

            let cf = StateKeeperColumnFamily::FactoryDeps;
            for factory_dep_hash in &factory_deps {
                batch.delete_cf(cf, factory_dep_hash.as_bytes());
            }

            db.write(batch)
                .expect("failed to save state data into RocksDB");
        })
        .await
        .unwrap();
    }

    /// Saves the pending changes to RocksDB. Must be executed on a Tokio thread.
    async fn save(&mut self, l1_batch_number: L1BatchNumber) {
        let pending_patch = mem::take(&mut self.pending_patch);

        let db = self.db.clone();
        let save_task = tokio::task::spawn_blocking(move || {
            let mut batch = db.new_write_batch();
            let cf = StateKeeperColumnFamily::State;
            batch.put_cf(
                cf,
                Self::BLOCK_NUMBER_KEY,
                &serialize_block_number(l1_batch_number.0),
            );
            for (key, (value, enum_index)) in pending_patch.state {
                batch.put_cf(
                    cf,
                    &Self::serialize_state_key(&key),
                    &StateValue::new(value, Some(enum_index)).serialize(),
                );
            }

            let cf = StateKeeperColumnFamily::FactoryDeps;
            for (hash, value) in pending_patch.factory_deps {
                batch.put_cf(cf, &hash.to_fixed_bytes(), value.as_ref());
            }
            db.write(batch)
                .expect("failed to save state data into rocksdb");
        });
        save_task.await.unwrap();
    }

    /// Returns the last processed l1 batch number + 1
    /// # Panics
    /// Panics on RocksDB errors.
    pub fn l1_batch_number(&self) -> L1BatchNumber {
        let cf = StateKeeperColumnFamily::State;
        let block_number = self
            .db
            .get_cf(cf, Self::BLOCK_NUMBER_KEY)
            .expect("failed to fetch block number");
        let block_number = block_number.map_or(0, |bytes| deserialize_block_number(&bytes));
        L1BatchNumber(block_number)
    }

    fn serialize_state_key(key: &StorageKey) -> [u8; 32] {
        key.hashed_key().to_fixed_bytes()
    }

    /// Estimates the number of key–value entries in the VM state.
    fn estimated_map_size(&self) -> u64 {
        self.db
            .estimated_number_of_entries(StateKeeperColumnFamily::State)
    }

    fn enum_migration_start_from(&self) -> Option<H256> {
        let value = self
            .db
            .get_cf(
                StateKeeperColumnFamily::State,
                Self::ENUM_INDEX_MIGRATION_CURSOR,
            )
            .expect("failed to read `ENUM_INDEX_MIGRATION_CURSOR`");
        match value {
            Some(v) if v.is_empty() => None,
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
        self.read_state_value(key)
            .map(|state_value| state_value.enum_index.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::test_utils::{
        create_l1_batch, create_miniblock, gen_storage_logs, prepare_postgres,
    };
    use zksync_dal::ConnectionPool;
    use zksync_types::{MiniblockNumber, StorageLog};

    #[tokio::test]
    async fn rocksdb_storage_basics() {
        let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
        let mut storage = RocksdbStorage::new(dir.path());
        let mut storage_logs: HashMap<_, _> = gen_storage_logs(0..20)
            .into_iter()
            .map(|log| (log.key, log.value))
            .collect();
        let changed_keys = storage.process_transaction_logs(storage_logs.clone());
        storage.pending_patch.state = changed_keys
            .map(|(key, state_value)| (key, (state_value.value, 1))) // enum index doesn't matter in the test
            .collect();
        storage.save(L1BatchNumber(0)).await;
        {
            for (key, value) in &storage_logs {
                assert!(!storage.is_write_initial(key));
                assert_eq!(storage.read_value(key), *value);
            }
        }

        // Overwrite some of the logs.
        for log in storage_logs.values_mut().step_by(2) {
            *log = StorageValue::zero();
        }
        let changed_keys = storage.process_transaction_logs(storage_logs.clone());
        storage.pending_patch.state = changed_keys
            .map(|(key, state_value)| (key, (state_value.value, 1))) // enum index doesn't matter in the test
            .collect();
        storage.save(L1BatchNumber(1)).await;

        for (key, value) in &storage_logs {
            assert!(!storage.is_write_initial(key));
            assert_eq!(storage.read_value(key), *value);
        }
    }

    #[tokio::test]
    async fn rocksdb_storage_syncing_with_postgres() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_miniblock(&mut conn, MiniblockNumber(1), storage_logs.clone()).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

        let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
        let mut storage = RocksdbStorage::new(dir.path());
        storage.update_from_postgres(&mut conn).await;

        assert_eq!(storage.l1_batch_number(), L1BatchNumber(2));
        for log in &storage_logs {
            assert_eq!(storage.read_value(&log.key), log.value);
        }
    }

    async fn insert_factory_deps(
        conn: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
        indices: impl Iterator<Item = u8>,
    ) {
        let factory_deps = indices
            .map(|i| (H256::repeat_byte(i), vec![i; 64]))
            .collect();
        conn.storage_dal()
            .insert_factory_deps(miniblock_number, &factory_deps)
            .await;
    }

    #[tokio::test]
    async fn rocksdb_storage_revert() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_miniblock(&mut conn, MiniblockNumber(1), storage_logs[..10].to_vec()).await;
        insert_factory_deps(&mut conn, MiniblockNumber(1), 0..1).await;
        create_miniblock(&mut conn, MiniblockNumber(2), storage_logs[10..].to_vec()).await;
        insert_factory_deps(&mut conn, MiniblockNumber(2), 1..3).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

        let inserted_storage_logs = gen_storage_logs(50..60);
        let replaced_storage_logs: Vec<_> = storage_logs
            .iter()
            .step_by(2)
            .map(|&log| StorageLog {
                value: H256::repeat_byte(0xf0),
                ..log
            })
            .collect();

        let mut new_storage_logs = inserted_storage_logs.clone();
        new_storage_logs.extend_from_slice(&replaced_storage_logs);
        create_miniblock(&mut conn, MiniblockNumber(3), new_storage_logs).await;
        insert_factory_deps(&mut conn, MiniblockNumber(3), 3..5).await;
        create_l1_batch(&mut conn, L1BatchNumber(2), &inserted_storage_logs).await;

        let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
        let mut storage = RocksdbStorage::new(dir.path());
        storage.update_from_postgres(&mut conn).await;

        // Perform some sanity checks before the revert.
        assert_eq!(storage.l1_batch_number(), L1BatchNumber(3));
        {
            for log in &inserted_storage_logs {
                assert_eq!(storage.read_value(&log.key), log.value);
            }
            for log in &replaced_storage_logs {
                assert_eq!(storage.read_value(&log.key), log.value);
            }

            for i in 0..5 {
                assert_eq!(
                    storage.load_factory_dep(H256::repeat_byte(i)).unwrap(),
                    [i; 64]
                );
            }
        }

        storage.rollback(&mut conn, L1BatchNumber(1)).await;
        assert_eq!(storage.l1_batch_number(), L1BatchNumber(2));
        {
            for log in &inserted_storage_logs {
                assert_eq!(storage.read_value(&log.key), H256::zero());
            }
            for log in &replaced_storage_logs {
                assert_ne!(storage.read_value(&log.key), log.value);
            }

            for i in 0..3 {
                assert_eq!(
                    storage.load_factory_dep(H256::repeat_byte(i)).unwrap(),
                    [i; 64]
                );
            }
            for i in 3..5 {
                assert!(storage.load_factory_dep(H256::repeat_byte(i)).is_none());
            }
        }
    }

    #[tokio::test]
    async fn rocksdb_enum_index_migration() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_miniblock(&mut conn, MiniblockNumber(1), storage_logs.clone()).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

        let enum_indices: HashMap<_, _> = conn
            .storage_logs_dedup_dal()
            .initial_writes_for_batch(L1BatchNumber(1))
            .await
            .into_iter()
            .collect();

        let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
        let mut storage = RocksdbStorage::new(dir.path());
        storage.update_from_postgres(&mut conn).await;

        assert_eq!(storage.l1_batch_number(), L1BatchNumber(2));
        // Check that enum indices are correct after syncing with postgres.
        for log in &storage_logs {
            let expected_index = enum_indices[&log.key.hashed_key()];
            assert_eq!(
                storage.read_state_value(&log.key).unwrap().enum_index,
                Some(expected_index)
            );
        }

        // Remove enum indices for some keys.
        let mut write_batch = storage.db.new_write_batch();
        for log in &storage_logs {
            write_batch.put_cf(
                StateKeeperColumnFamily::State,
                log.key.hashed_key().as_bytes(),
                log.value.as_bytes(),
            );
            write_batch.delete_cf(
                StateKeeperColumnFamily::State,
                RocksdbStorage::ENUM_INDEX_MIGRATION_CURSOR,
            );
        }
        storage.db.write(write_batch).unwrap();

        // Check that migration works as expected.
        let ordered_keys_to_migrate: Vec<StorageKey> = storage_logs
            .iter()
            .map(|log| log.key)
            .sorted_by_key(StorageKey::hashed_key)
            .collect();

        storage.enable_enum_index_migration(10);
        let start_from = storage.enum_migration_start_from();
        assert_eq!(start_from, Some(H256::zero()));

        // Migrate the first half.
        storage.save_missing_enum_indices(&mut conn).await;
        for key in ordered_keys_to_migrate.iter().take(10) {
            let expected_index = enum_indices[&key.hashed_key()];
            assert_eq!(
                storage.read_state_value(key).unwrap().enum_index,
                Some(expected_index)
            );
        }
        assert!(storage
            .read_state_value(&ordered_keys_to_migrate[10])
            .unwrap()
            .enum_index
            .is_none());

        // Migrate the second half.
        storage.save_missing_enum_indices(&mut conn).await;
        for key in ordered_keys_to_migrate.iter().skip(10) {
            let expected_index = enum_indices[&key.hashed_key()];
            assert_eq!(
                storage.read_state_value(key).unwrap().enum_index,
                Some(expected_index)
            );
        }

        // 20 keys were processed but we haven't checked that no keys to migrate are left.
        let start_from = storage.enum_migration_start_from();
        assert!(start_from.is_some());

        // Check that migration will be marked as completed after the next iteration.
        storage.save_missing_enum_indices(&mut conn).await;
        let start_from = storage.enum_migration_start_from();
        assert!(start_from.is_none());
    }
}

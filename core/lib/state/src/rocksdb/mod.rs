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
//! | Column       | Key                    | Value                   | Description                          |
//! | ------------ | ---------------------- | ----------------------- | ------------------------------------ |
//! | State        | 'block_number'         | serialized block number | Last processed L1 batch number (u32) |
//! | State        | hashed `StorageKey`    | 32 bytes value          | State for the given key              |
//! | Contracts    | address (20 bytes)     | `Vec<u8>`               | Contract contents                    |
//! | Factory deps | hash (32 bytes)        | `Vec<u8>`               | Bytecodes for new contracts that a certain contract may deploy. |

use std::{collections::HashMap, convert::TryInto, mem, path::Path, time::Instant};

use zksync_dal::StorageProcessor;
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};

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

/// [`ReadStorage`] implementation backed by RocksDB.
#[derive(Debug)]
pub struct RocksdbStorage {
    db: RocksDB<StateKeeperColumnFamily>,
    pending_patch: InMemoryStorage,
    enum_index_migration_chunks: u16,
}

impl RocksdbStorage {
    const BLOCK_NUMBER_KEY: &'static [u8] = b"block_number";
    const ENUM_INDEX_MIGRATION_PROGRESS_KEY: &'static [u8] = b"enum_index_migration_progress";
    const ENUM_INDEX_MIGRATION_FINISHED: &'static [u8] = b"enum_index_migration_finished";

    /// Creates a new storage with the provided RocksDB `path`.
    pub fn new(path: &Path) -> Self {
        let db = RocksDB::new(path, true);
        Self {
            db,
            pending_patch: InMemoryStorage::default(),
            enum_index_migration_chunks: 0,
        }
    }

    /// Creates a new storage with the provided RocksDB `path` and enum index migration settings.
    pub fn new_with_enum_index_migration_chunks(
        path: &Path,
        enum_index_migration_chunks: u16,
    ) -> Self {
        let db = RocksDB::new(path, true);
        Self {
            db,
            pending_patch: InMemoryStorage::default(),
            enum_index_migration_chunks,
        }
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
            let changed_keys = self.process_transaction_logs(storage_logs);

            tracing::debug!("Loading enumeration indexes for l1 batch {current_l1_batch_number}");
            let enum_indices: HashMap<H256, u64> = conn
                .storage_logs_dedup_dal()
                .initial_writes_for_batch(L1BatchNumber(current_l1_batch_number))
                .await
                .into_iter()
                .collect();
            self.pending_patch.state = changed_keys
                .into_iter()
                .map(|(key, (value, index))| {
                    let index = index.unwrap_or_else(|| {
                        enum_indices
                            .get(&key.hashed_key())
                            .copied()
                            .unwrap_or_else(|| {
                                panic!(
                                    "Missing enumeration index for hashed key {:?}",
                                    key.hashed_key()
                                )
                            })
                    });
                    (key, (value, index))
                })
                .collect();

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

        self.save_missing_enum_indices(conn).await;
    }

    async fn save_missing_enum_indices(&self, conn: &mut StorageProcessor<'_>) {
        if let (Some(start_from), true) = (
            self.enum_migration_start_from(),
            self.enum_index_migration_chunks > 0,
        ) {
            let started_at = Instant::now();
            let chunk_from = u16::from_be_bytes(start_from);
            let chunk_to = chunk_from.saturating_add(self.enum_index_migration_chunks - 1);
            tracing::info!("RocksDB enum index migration is not finished, processing chunks {chunk_from}..={chunk_to}");

            let mut write_batch = self.db.new_write_batch();
            let mut number_of_keys_migrated = 0;
            for chunk in chunk_from..=chunk_to {
                let (keys, values): (Vec<_>, Vec<_>) = self
                    .db
                    .prefix_iterator_cf(StateKeeperColumnFamily::State, &chunk.to_be_bytes())
                    .filter_map(|(key, value)| {
                        (key.as_ref() != Self::BLOCK_NUMBER_KEY
                            && key.as_ref() != Self::ENUM_INDEX_MIGRATION_PROGRESS_KEY
                            && key.as_ref() != Self::ENUM_INDEX_MIGRATION_FINISHED)
                            .then(|| (H256::from_slice(&key), H256::from_slice(&value[..32])))
                    })
                    .unzip();
                let indices = conn
                    .storage_logs_dedup_dal()
                    .enum_indices_for_keys(&keys)
                    .await;
                number_of_keys_migrated += indices.len();
                for ((key, value), index) in keys.into_iter().zip(values).zip(indices) {
                    write_batch.put_cf(
                        StateKeeperColumnFamily::State,
                        key.as_bytes(),
                        &Self::serialize_state_value(value, index),
                    );
                }
            }

            if chunk_to == u16::MAX {
                write_batch.put_cf(
                    StateKeeperColumnFamily::State,
                    Self::ENUM_INDEX_MIGRATION_FINISHED,
                    &[1u8; 1],
                );
                write_batch.delete_cf(
                    StateKeeperColumnFamily::State,
                    Self::ENUM_INDEX_MIGRATION_PROGRESS_KEY,
                );
            } else {
                write_batch.put_cf(
                    StateKeeperColumnFamily::State,
                    Self::ENUM_INDEX_MIGRATION_PROGRESS_KEY,
                    &(chunk_to + 1).to_be_bytes(),
                );
            }
            self.db
                .write(write_batch)
                .expect("failed to save state data into rocksdb");
            tracing::info!(
                "RocksDB enum index migration for chunks {chunk_from}..={chunk_to} took {:?}, migrated {number_of_keys_migrated} keys",
                started_at.elapsed()
            );
        }
    }

    fn read_value_inner(&self, key: &StorageKey) -> Option<StorageValue> {
        let cf = StateKeeperColumnFamily::State;
        self.db
            .get_cf(cf, &Self::serialize_state_key(key))
            .expect("failed to read rocksdb state value")
            .map(|value| H256::from_slice(&value[..32]))
    }

    fn read_enum_index(&self, key: &StorageKey) -> Option<u64> {
        let cf = StateKeeperColumnFamily::State;
        self.db
            .get_cf(cf, &Self::serialize_state_key(key))
            .expect("failed to read rocksdb state value")
            .and_then(|value| {
                if value.len() == 40 {
                    Some(u64::from_be_bytes(value[32..].try_into().unwrap()))
                } else {
                    None
                }
            })
    }

    /// Processes storage `logs` produced by transactions.
    fn process_transaction_logs(
        &self,
        updates: HashMap<StorageKey, H256>,
    ) -> HashMap<StorageKey, (StorageValue, Option<u64>)> {
        updates
            .into_iter()
            .filter_map(|(key, value)| {
                if self.read_value_inner(&key).is_some() {
                    Some((key, (value, self.read_enum_index(&key))))
                } else {
                    (!value.is_zero()).then_some((key, (value, None)))
                }
            })
            .collect()
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
                if let Some(prev_value) = maybe_value {
                    batch.put_cf(cf, key.as_bytes(), prev_value.as_bytes());
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
                    &Self::serialize_state_value(value, enum_index),
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

    fn serialize_state_value(value: H256, enum_index: u64) -> Vec<u8> {
        value
            .0
            .into_iter()
            .chain(enum_index.to_be_bytes())
            .collect()
    }

    /// Estimates the number of key–value entries in the VM state.
    fn estimated_map_size(&self) -> u64 {
        self.db
            .estimated_number_of_entries(StateKeeperColumnFamily::State)
    }

    fn enum_migration_start_from(&self) -> Option<[u8; 2]> {
        let finished = self
            .db
            .get_cf(
                StateKeeperColumnFamily::State,
                Self::ENUM_INDEX_MIGRATION_FINISHED,
            )
            .expect("failed to read `ENUM_INDEX_MIGRATION_FINISHED`")
            .is_some();
        (!finished).then(|| {
            self.db
                .get_cf(
                    StateKeeperColumnFamily::State,
                    Self::ENUM_INDEX_MIGRATION_PROGRESS_KEY,
                )
                .expect("failed to read `ENUM_INDEX_MIGRATION_FINISHED`")
                .map_or([0u8; 2], |v| v.try_into().unwrap())
        })
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
}

#[cfg(test)]
mod tests {
    use db_test_macro::db_test;
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
            .into_iter()
            .map(|(key, (value, _))| (key, (value, 1))) // enum index doesn't matter in the test
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
            .into_iter()
            .map(|(key, (value, _))| (key, (value, 1))) // enum index doesn't matter in the test
            .collect();
        storage.save(L1BatchNumber(1)).await;

        for (key, value) in &storage_logs {
            assert!(!storage.is_write_initial(key));
            assert_eq!(storage.read_value(key), *value);
        }
    }

    #[db_test]
    async fn rocksdb_storage_syncing_with_postgres(pool: ConnectionPool) {
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

    #[db_test]
    async fn rocksdb_storage_revert(pool: ConnectionPool) {
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
}

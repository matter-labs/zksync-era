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

use std::{collections::HashMap, mem, path::Path, sync::Arc, time::Instant};

use crate::{InMemoryStorage, ReadStorage};
use zksync_dal::StorageProcessor;
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{L1BatchNumber, StorageKey, StorageValue, H256};

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
    db: Arc<RocksDB<StateKeeperColumnFamily>>,
    pending_patch: InMemoryStorage,
}

impl RocksdbStorage {
    const BLOCK_NUMBER_KEY: &'static [u8] = b"block_number";

    /// Creates a new storage with the provided RocksDB `path`.
    pub fn new(path: &Path) -> Self {
        let db = RocksDB::new(path, true);
        Self {
            db: Arc::new(db),
            pending_patch: InMemoryStorage::default(),
        }
    }

    /// Synchronizes this storage with Postgres using the provided connection.
    ///
    /// # Panics
    ///
    /// Panics if the local L1 batch number is greater than the last sealed L1 batch number
    /// in Postgres.
    pub async fn update_from_postgres(&mut self, conn: &mut StorageProcessor<'_>) {
        let stage_started_at: Instant = Instant::now();
        let latest_l1_batch_number = conn.blocks_dal().get_sealed_l1_batch_number().await;
        vlog::debug!(
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
            vlog::debug!("loading state changes for l1 batch {current_l1_batch_number}");
            let storage_logs = conn
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(L1BatchNumber(current_l1_batch_number))
                .await;
            self.process_transaction_logs(&storage_logs);

            vlog::debug!("loading factory deps for l1 batch {current_l1_batch_number}");
            let factory_deps = conn
                .blocks_dal()
                .get_l1_batch_factory_deps(L1BatchNumber(current_l1_batch_number))
                .await;
            for (hash, bytecode) in factory_deps {
                self.store_factory_dep(hash, bytecode);
            }

            current_l1_batch_number += 1;
            self.save(L1BatchNumber(current_l1_batch_number)).await;
        }

        metrics::histogram!(
            "server.state_keeper.update_secondary_storage",
            stage_started_at.elapsed()
        );
    }

    fn read_value_inner(&self, key: &StorageKey) -> Option<StorageValue> {
        let cf = StateKeeperColumnFamily::State;
        self.db
            .get_cf(cf, &Self::serialize_state_key(key))
            .expect("failed to read rocksdb state value")
            .map(|value| H256::from_slice(&value))
    }

    /// Processes storage `logs` produced by transactions.
    fn process_transaction_logs(&mut self, updates: &HashMap<StorageKey, H256>) {
        for (&key, &value) in updates {
            if !value.is_zero() || self.read_value_inner(&key).is_some() {
                self.pending_patch.state.insert(key, value);
            }
        }
    }

    /// Stores a factory dependency with the specified `hash` and `bytecode`.
    pub fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
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
        vlog::info!("Rolling back state keeper storage to L1 batch #{last_l1_batch_to_keep}...");

        vlog::info!("Getting logs that should be applied to rollback state...");
        let stage_start = Instant::now();
        let logs = connection
            .storage_logs_dal()
            .get_storage_logs_for_revert(last_l1_batch_to_keep)
            .await;
        vlog::info!("Got {} logs, took {:?}", logs.len(), stage_start.elapsed());

        vlog::info!("Getting number of last miniblock for L1 batch #{last_l1_batch_to_keep}...");
        let stage_start = Instant::now();
        let (_, last_miniblock_to_keep) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(last_l1_batch_to_keep)
            .await
            .expect("L1 batch should contain at least one miniblock");
        vlog::info!(
            "Got miniblock number {last_miniblock_to_keep}, took {:?}",
            stage_start.elapsed()
        );

        vlog::info!("Getting factory deps that need to be removed...");
        let stage_start = Instant::now();
        let factory_deps = connection
            .storage_dal()
            .get_factory_deps_for_revert(last_miniblock_to_keep)
            .await;
        vlog::info!(
            "Got {} factory deps, took {:?}",
            factory_deps.len(),
            stage_start.elapsed()
        );

        let db = Arc::clone(&self.db);
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

        let db = Arc::clone(&self.db);
        let save_task = tokio::task::spawn_blocking(move || {
            let mut batch = db.new_write_batch();
            let cf = StateKeeperColumnFamily::State;
            batch.put_cf(
                cf,
                Self::BLOCK_NUMBER_KEY,
                &serialize_block_number(l1_batch_number.0),
            );
            for (key, value) in pending_patch.state {
                batch.put_cf(cf, &Self::serialize_state_key(&key), value.as_ref());
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
    pub fn estimated_map_size(&self) -> u64 {
        self.db
            .estimated_number_of_entries(StateKeeperColumnFamily::State)
    }
}

impl ReadStorage for &RocksdbStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.read_value_inner(key).unwrap_or_else(H256::zero)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.read_value_inner(key).is_none()
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        if let Some(value) = self.pending_patch.factory_deps.get(&hash) {
            return Some(value.clone());
        }
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
        let mut storage_logs = gen_storage_logs(0..20)
            .into_iter()
            .map(|log| (log.key, log.value))
            .collect();
        storage.process_transaction_logs(&storage_logs);
        storage.save(L1BatchNumber(0)).await;
        {
            let mut storage = &storage;
            for (key, value) in &storage_logs {
                assert!(!storage.is_write_initial(key));
                assert_eq!(storage.read_value(key), *value);
            }
        }

        // Overwrite some of the logs.
        for log in storage_logs.values_mut().step_by(2) {
            *log = StorageValue::zero();
        }
        storage.process_transaction_logs(&storage_logs);
        storage.save(L1BatchNumber(1)).await;

        let mut storage = &storage;
        for (key, value) in &storage_logs {
            assert!(!storage.is_write_initial(key));
            assert_eq!(storage.read_value(key), *value);
        }
    }

    #[db_test]
    async fn rocksdb_storage_syncing_with_postgres(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await;
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_miniblock(&mut conn, MiniblockNumber(1), storage_logs.clone()).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

        let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
        let mut storage = RocksdbStorage::new(dir.path());
        storage.update_from_postgres(&mut conn).await;

        let mut storage = &storage;
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
        let mut conn = pool.access_storage().await;
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
            let mut storage = &storage;
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
            let mut storage = &storage;
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

use std::collections::HashMap;
use std::ops::Deref;
use zksync_storage::db::StateKeeperColumnFamily;
use zksync_storage::rocksdb::WriteBatch;
use zksync_storage::util::{deserialize_block_number, serialize_block_number};
use zksync_storage::RocksDB;
use zksync_types::{
    L1BatchNumber, StorageKey, StorageLog, StorageLogKind, StorageValue, ZkSyncReadStorage, H256,
};

const BLOCK_NUMBER_KEY: &[u8; 12] = b"block_number";

#[derive(Debug)]
pub struct SecondaryStateStorage {
    db: RocksDB,
    // currently not used
    pending_patch: PendingPatch,
}

#[derive(Default, Debug)]
struct PendingPatch {
    state: HashMap<StorageKey, [u8; 32]>,
    factory_deps: HashMap<H256, Vec<u8>>,
}

impl ZkSyncReadStorage for &SecondaryStateStorage {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        self.read_value_inner(key).unwrap_or_else(H256::zero)
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        self.read_value_inner(key).is_none()
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        self.load_factory_dependency(hash)
    }
}

impl SecondaryStateStorage {
    pub fn new(db: RocksDB) -> Self {
        Self {
            db,
            pending_patch: PendingPatch::default(),
        }
    }

    fn read_value_inner(&self, key: &StorageKey) -> Option<StorageValue> {
        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::State);
        self.db
            .get_cf(cf, SecondaryStateStorage::serialize_state_key(key))
            .expect("failed to read rocksdb state value")
            .map(|value| H256::from_slice(&value))
    }

    pub fn load_factory_dependency(&self, hash: H256) -> Option<Vec<u8>> {
        if let Some(value) = self.pending_patch.factory_deps.get(&hash) {
            return Some(value.clone());
        }
        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::FactoryDeps);
        self.db
            .get_cf(cf, hash.to_fixed_bytes())
            .expect("failed to read rocksdb state value")
    }

    pub fn process_transaction_logs(&mut self, logs: &[StorageLog]) {
        let mut updates = HashMap::new();
        for log in logs {
            if log.kind == StorageLogKind::Write {
                updates.insert(log.key, log.value.to_fixed_bytes());
            }
        }
        for (key, value) in updates {
            if value != [0u8; 32] || self.deref().read_value_inner(&key).is_some() {
                self.pending_patch.state.insert(key, value);
            }
        }
    }

    pub fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>) {
        self.pending_patch.factory_deps.insert(hash, bytecode);
    }

    pub fn rollback(
        &mut self,
        logs: Vec<(H256, Option<H256>)>,
        factory_deps: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) {
        let mut batch = WriteBatch::default();

        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::State);
        for (key, value) in logs {
            match value {
                Some(value) => batch.put_cf(cf, key.0, value.to_fixed_bytes()),
                None => batch.delete_cf(cf, key.0),
            }
        }
        batch.put_cf(
            cf,
            BLOCK_NUMBER_KEY,
            serialize_block_number(l1_batch_number.0 + 1),
        );

        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::FactoryDeps);
        for factory_dep_hash in factory_deps {
            batch.delete_cf(cf, factory_dep_hash.to_fixed_bytes());
        }

        self.db
            .write(batch)
            .expect("failed to save state data into rocksdb");
    }

    pub fn save(&mut self, l1_batch_number: L1BatchNumber) {
        let mut batch = WriteBatch::default();

        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::State);
        batch.put_cf(
            cf,
            BLOCK_NUMBER_KEY,
            serialize_block_number(l1_batch_number.0),
        );
        for (key, value) in self.pending_patch.state.iter() {
            batch.put_cf(cf, Self::serialize_state_key(key), value);
        }

        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::FactoryDeps);
        for (hash, value) in self.pending_patch.factory_deps.iter() {
            batch.put_cf(cf, hash.to_fixed_bytes(), value);
        }

        self.db
            .write(batch)
            .expect("failed to save state data into rocksdb");
        self.pending_patch = PendingPatch::default();
    }

    /// Returns the last processed l1 batch number + 1
    pub fn get_l1_batch_number(&self) -> L1BatchNumber {
        let cf = self
            .db
            .cf_state_keeper_handle(StateKeeperColumnFamily::State);
        let block_number = self
            .db
            .get_cf(cf, BLOCK_NUMBER_KEY)
            .expect("failed to fetch block number")
            .map(|bytes| deserialize_block_number(&bytes))
            .unwrap_or(0);
        L1BatchNumber(block_number)
    }

    fn serialize_state_key(key: &StorageKey) -> Vec<u8> {
        key.hashed_key().to_fixed_bytes().into()
    }

    pub fn get_estimated_map_size(&self) -> u64 {
        self.db
            .get_estimated_number_of_entries(StateKeeperColumnFamily::State)
    }
}

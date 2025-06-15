use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use dashmap::DashMap;
use zk_ee::utils::Bytes32;
use zksync_storage::db::NamedColumnFamily;
use zksync_storage::RocksDB;
use crate::storage::storage_map;
use crate::execution::metrics::{STORAGE_MAP_ROCKS_DB_METRICS};

/// Wrapper for map of storage diffs that are persisted in RocksDB.
///
/// Cheaply clonable / thread safe
#[derive(Debug, Clone)]
pub struct PersistentStorageMap {
    /// RocksDB handle for the persistent base - cheap to clone
    pub rocks: RocksDB<StorageMapCF>,
}

#[derive(Clone, Copy, Debug)]
pub enum StorageMapCF {
    Storage,
    Meta,
}

impl NamedColumnFamily for StorageMapCF {
    const DB_NAME: &'static str = "storage_map";
    const ALL: &'static [Self] = &[StorageMapCF::Storage, StorageMapCF::Meta];

    fn name(&self) -> &'static str {
        match self {
            StorageMapCF::Storage => "storage",
            StorageMapCF::Meta => "meta",
        }
    }
}

impl StorageMapCF {
    fn base_block_key() -> &'static [u8] {
        b"base_block"
    }
}

impl PersistentStorageMap {
    pub fn new(rocks: RocksDB<StorageMapCF>) -> Self {
        Self {
            rocks,
        }
    }

    pub fn rocksdb_block_number(&self) -> u64 {
        self.rocks
            .get_cf(StorageMapCF::Meta, StorageMapCF::base_block_key())
            .ok()
            .flatten()
            .map(|v| u64::from_be_bytes(v.as_slice().try_into().unwrap()))
            .unwrap_or(0)
    }

    pub fn get_from_rocks(&self, key: Bytes32) -> Option<Bytes32> {
        let latency = STORAGE_MAP_ROCKS_DB_METRICS.get[&"total"].start();
        let res = self.rocks
            .get_cf(StorageMapCF::Storage, key.as_u8_array_ref())
            .ok()
            .flatten()
            .map(|bytes| {
                let arr: [u8; 32] = bytes.as_slice()
                    .try_into()                  // Vec<u8> → [u8; 32]
                    .expect("value must be 32 bytes");
                Bytes32::from(arr)
            });
        latency.observe();
        res
    }

    pub fn compact_sync(&self, new_block_number: u64, diffs: HashMap<Bytes32, Bytes32>) {
        let latency = STORAGE_MAP_ROCKS_DB_METRICS.compact[&"total"].start();

        let mut batch = self.rocks.new_write_batch();

        for (k, v) in diffs {
            batch.put_cf(StorageMapCF::Storage, k.as_u8_array_ref(), v.as_u8_array_ref());
        }
        batch.put_cf(
            StorageMapCF::Meta,
            StorageMapCF::base_block_key(),
            new_block_number.to_be_bytes().as_ref(),
        );

        self.rocks.write(batch).expect("RocksDB write failed");
        latency.observe();
    }
}
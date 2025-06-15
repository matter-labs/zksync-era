use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::PreimageSource;
use zksync_storage::db::NamedColumnFamily;
use zksync_storage::RocksDB;
use crate::storage::persistent_storage_map::StorageMapCF;
use crate::execution::metrics::{PREIMAGES_ROCKS_DB_METRICS};

#[derive(Clone, Debug)]
pub struct RocksDbPreimages {
    /// RocksDB handle for the persistent base - cheap to clone
    pub rocks: RocksDB<PreimagesCF>,
}

#[derive(Clone, Copy, Debug)]
pub enum PreimagesCF {
    Storage,
    Meta,
}


impl NamedColumnFamily for PreimagesCF {
    const DB_NAME: &'static str = "preimages";
    const ALL: &'static [Self] = &[PreimagesCF::Storage, PreimagesCF::Meta];

    fn name(&self) -> &'static str {
        match self {
            PreimagesCF::Storage => "storage",
            PreimagesCF::Meta => "meta",
        }
    }
}

impl PreimagesCF {
    fn block_key() -> &'static [u8] {
        b"block"
    }
}


impl RocksDbPreimages {
    pub fn new(rocks: RocksDB<PreimagesCF>) -> Self {
        Self {
            rocks,
        }
    }


    pub fn rocksdb_block_number(&self) -> u64 {
        self.rocks
            .get_cf(PreimagesCF::Meta, PreimagesCF::block_key())
            .ok()
            .flatten()
            .map(|v| u64::from_be_bytes(v.as_slice().try_into().unwrap()))
            .unwrap_or(0)
    }

    /// Insert multiple preimages at once.
    ///
    /// Each `(key, preimage)` is added if the key is not already present.
    /// This batch insertion is safe for concurrent use.
    pub fn get(&self, key: Bytes32) -> Option<Vec<u8>> {
        let latency = PREIMAGES_ROCKS_DB_METRICS.get_latency[&"total"].start();
        let res = self.rocks
            .get_cf(PreimagesCF::Storage, key.as_u8_array_ref())
            .ok()
            .flatten();
        latency.observe();
        res
    }

    pub fn add<I>(&self, new_block_number: u64, diffs: I)
    where
        I: IntoIterator<Item=(Bytes32, Vec<u8>)>,
    {
        let latency = PREIMAGES_ROCKS_DB_METRICS.set_latency[&"total"].start();

        let mut batch = self.rocks.new_write_batch();

        for (k, v) in diffs {
            batch.put_cf(PreimagesCF::Storage, k.as_u8_array_ref(), &v);
        }
        batch.put_cf(
            PreimagesCF::Meta,
            PreimagesCF::block_key(),
            new_block_number.to_be_bytes().as_ref(),
        );

        self.rocks.write(batch).expect("RocksDB write failed");
        latency.observe();
    }
}


impl PreimageSource for RocksDbPreimages {
    fn get_preimage(&mut self, hash: Bytes32) -> Option<Vec<u8>> {
        self.get(hash)
    }
}
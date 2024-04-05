use async_trait::async_trait;
use std::fmt::Debug;
use tokio::sync::watch;
use zksync_types::L1BatchNumber;

use crate::{PostgresStorage, ReadStorage, RocksdbStorage};

/// Factory that can produce a [`ReadStorage`] implementation on demand.
#[async_trait]
pub trait ReadStorageFactory: Debug + Send + Sync + 'static {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>>;
}

/// A [`ReadStorage`] implementation that uses either [`PostgresStorage`] or [`RocksdbStorage`]
/// underneath.
#[derive(Debug)]
pub enum PgOrRocksdbStorage<'a> {
    Postgres(PostgresStorage<'a>),
    Rocksdb(RocksdbStorage),
}

impl ReadStorage for PgOrRocksdbStorage<'_> {
    fn read_value(&mut self, key: &zksync_types::StorageKey) -> zksync_types::StorageValue {
        match self {
            Self::Postgres(postgres) => postgres.read_value(key),
            Self::Rocksdb(rocksdb) => rocksdb.read_value(key),
        }
    }

    fn is_write_initial(&mut self, key: &zksync_types::StorageKey) -> bool {
        match self {
            Self::Postgres(postgres) => postgres.is_write_initial(key),
            Self::Rocksdb(rocksdb) => rocksdb.is_write_initial(key),
        }
    }

    fn load_factory_dep(&mut self, hash: zksync_types::H256) -> Option<Vec<u8>> {
        match self {
            Self::Postgres(postgres) => postgres.load_factory_dep(hash),
            Self::Rocksdb(rocksdb) => rocksdb.load_factory_dep(hash),
        }
    }

    fn get_enumeration_index(&mut self, key: &zksync_types::StorageKey) -> Option<u64> {
        match self {
            Self::Postgres(postgres) => postgres.get_enumeration_index(key),
            Self::Rocksdb(rocksdb) => rocksdb.get_enumeration_index(key),
        }
    }
}

impl<'a> From<PostgresStorage<'a>> for PgOrRocksdbStorage<'a> {
    fn from(value: PostgresStorage<'a>) -> Self {
        Self::Postgres(value)
    }
}

impl<'a> From<RocksdbStorage> for PgOrRocksdbStorage<'a> {
    fn from(value: RocksdbStorage) -> Self {
        Self::Rocksdb(value)
    }
}

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{PgOrRocksdbStorage, ReadStorageFactory, RocksdbStorage};
use zksync_types::L1BatchNumber;

#[derive(Debug, Clone)]
pub struct PostgresFactory {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl ReadStorageFactory for PostgresFactory {
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        Ok(Some(
            PgOrRocksdbStorage::access_storage_pg(&self.pool, l1_batch_number).await?,
        ))
    }
}

impl PostgresFactory {
    pub fn new(pool: ConnectionPool<Core>) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
pub struct RocksdbFactory {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
}

#[async_trait]
impl ReadStorageFactory for RocksdbFactory {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        let builder = RocksdbStorage::builder(self.state_keeper_db_path.as_ref())
            .await
            .context("Failed opening state keeper RocksDB")?;
        let mut conn = self
            .pool
            .connection_tagged("state_keeper")
            .await
            .context("Failed getting a connection to Postgres")?;
        let Some(rocksdb_storage) = builder
            .synchronize(&mut conn, stop_receiver, None)
            .await
            .context("Failed synchronizing state keeper's RocksDB to Postgres")?
        else {
            return Ok(None);
        };
        Ok(Some(PgOrRocksdbStorage::Rocksdb(rocksdb_storage)))
    }
}

impl RocksdbFactory {
    pub fn new(pool: ConnectionPool<Core>, state_keeper_db_path: String) -> Self {
        Self {
            pool,
            state_keeper_db_path,
        }
    }
}

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{OwnedStorage, ReadStorageFactory, RocksdbStorage};
use zksync_types::L1BatchNumber;

#[derive(Debug, Clone)]
pub struct RocksdbStorageFactory {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
}

#[async_trait]
impl ReadStorageFactory for RocksdbStorageFactory {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<OwnedStorage>> {
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
        Ok(Some(OwnedStorage::Rocksdb(rocksdb_storage)))
    }
}

impl RocksdbStorageFactory {
    pub fn new(pool: ConnectionPool<Core>, state_keeper_db_path: String) -> Self {
        Self {
            pool,
            state_keeper_db_path,
        }
    }
}

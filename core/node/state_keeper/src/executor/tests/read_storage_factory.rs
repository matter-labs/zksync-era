use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{OwnedStorage, ReadStorageFactory, RocksdbStorage};
use zksync_types::{L1BatchNumber, OrStopped};

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
    ) -> Result<OwnedStorage, OrStopped> {
        // Waiting for recovery here is not a good idea in the general case because of snapshot recovery taking potentially very long time.
        // Since this is a test implementation, it's OK here.
        let (rocksdb, _) = RocksdbStorage::builder(self.state_keeper_db_path.as_ref())
            .await
            .context("Failed opening state keeper RocksDB")?
            .ensure_ready(&self.pool, stop_receiver)
            .await?;
        let mut conn = self
            .pool
            .connection_tagged("state_keeper")
            .await
            .context("Failed getting a connection to Postgres")?;
        let rocksdb_storage = rocksdb.synchronize(&mut conn, stop_receiver, None).await?;
        Ok(OwnedStorage::Rocksdb(rocksdb_storage))
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

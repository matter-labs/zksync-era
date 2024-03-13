use anyhow::Context;
use async_trait::async_trait;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::ConnectionPool;
use zksync_state::{open_state_keeper_rocksdb, PostgresStorage, RocksdbStorageBuilder};

use crate::state_keeper::state_keeper_storage::{PgOrRocksdbStorage, ReadStorageFactory};

#[derive(Debug, Clone)]
pub struct PostgresFactory {
    pool: ConnectionPool,
}

#[async_trait]
impl ReadStorageFactory for PostgresFactory {
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        let mut connection = self
            .pool
            .access_storage()
            .await
            .context("Failed accessing Postgres storage")?;

        let snapshot_recovery = connection
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .context("failed getting snapshot recovery info")?;
        let miniblock_number = if let Some(snapshot_recovery) = snapshot_recovery {
            snapshot_recovery.miniblock_number
        } else {
            let mut dal = connection.blocks_dal();
            let l1_batch_number = dal.get_sealed_l1_batch_number().await?.unwrap_or_default();
            let (_, miniblock_number) = dal
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .await?
                .unwrap_or_default();
            miniblock_number
        };

        Ok(Some(PgOrRocksdbStorage::Postgres(
            PostgresStorage::new_async(Handle::current(), connection, miniblock_number, true)
                .await?,
        )))
    }
}

impl PostgresFactory {
    pub fn new(pool: ConnectionPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
pub struct RocksdbFactory {
    pool: ConnectionPool,
    state_keeper_db_path: String,
    enum_index_migration_chunk_size: usize,
}

#[async_trait]
impl ReadStorageFactory for RocksdbFactory {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        let mut builder: RocksdbStorageBuilder =
            open_state_keeper_rocksdb(self.state_keeper_db_path.clone().into())
                .await
                .context("Failed opening state keeper RocksDB")?
                .into();
        builder.enable_enum_index_migration(self.enum_index_migration_chunk_size);
        let mut conn = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .context("Failed getting a connection to Postgres")?;
        let Some(rocksdb_storage) = builder
            .synchronize(&mut conn, stop_receiver)
            .await
            .context("Failed synchronizing state keeper's RocksDB to Postgres")?
        else {
            return Ok(None);
        };
        Ok(Some(PgOrRocksdbStorage::Rocksdb(rocksdb_storage)))
    }
}

impl RocksdbFactory {
    pub fn new(
        pool: ConnectionPool,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> Self {
        Self {
            pool,
            state_keeper_db_path,
            enum_index_migration_chunk_size,
        }
    }
}

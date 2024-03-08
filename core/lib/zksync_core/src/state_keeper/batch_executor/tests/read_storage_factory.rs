use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::ConnectionPool;
use zksync_state::{
    open_state_keeper_rocksdb, PostgresStorage, RocksdbStorage, RocksdbStorageBuilder,
};

use crate::state_keeper::{state_keeper_storage::ReadStorageFactory, StateKeeperStorage};

#[derive(Debug, Clone)]
pub struct PostgresFactory {
    pool: ConnectionPool,
}

#[async_trait]
impl ReadStorageFactory for PostgresFactory {
    type ReadStorageImpl<'a> = PostgresStorage<'a>;

    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        _stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>> {
        let mut connection = self.pool.access_storage().await.unwrap();

        let snapshot_recovery = connection
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .unwrap();
        let miniblock_number = if let Some(snapshot_recovery) = snapshot_recovery {
            snapshot_recovery.miniblock_number
        } else {
            let mut dal = connection.blocks_dal();
            let l1_batch_number = dal
                .get_sealed_l1_batch_number()
                .await
                .unwrap()
                .unwrap_or_default();
            let (_, miniblock_number) = dal
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .await
                .unwrap()
                .unwrap_or_default();
            miniblock_number
        };

        Some(
            PostgresStorage::new_async(rt_handle, connection, miniblock_number, true)
                .await
                .unwrap(),
        )
    }
}

impl StateKeeperStorage<PostgresFactory> {
    pub fn postgres(pool: ConnectionPool) -> Self {
        StateKeeperStorage::new(Arc::new(Mutex::new(PostgresFactory { pool })))
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
    type ReadStorageImpl<'a> = RocksdbStorage;

    async fn access_storage<'a>(
        &'a self,
        _rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>> {
        let mut builder: RocksdbStorageBuilder =
            open_state_keeper_rocksdb(self.state_keeper_db_path.clone().into())
                .await
                .expect("Failed initializing state keeper storage")
                .into();
        builder.enable_enum_index_migration(self.enum_index_migration_chunk_size);
        let mut conn = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();
        builder
            .synchronize(&mut conn, stop_receiver)
            .await
            .expect("Failed synchronizing secondary state keeper storage")
    }
}

impl StateKeeperStorage<RocksdbFactory> {
    pub fn rocksdb(
        pool: ConnectionPool,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> Self {
        StateKeeperStorage::new(Arc::new(Mutex::new(RocksdbFactory {
            pool,
            state_keeper_db_path,
            enum_index_migration_chunk_size,
        })))
    }
}

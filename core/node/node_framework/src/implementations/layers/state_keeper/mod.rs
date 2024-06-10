use std::{num::NonZero, sync::Arc};

use anyhow::Context;
use zksync_state::{AsyncCatchupTask, ReadStorageFactory, RocksdbStorageOptions};
use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, AsyncRocksdbCache, BatchExecutor, OutputHandler,
    StateKeeperIO, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;

pub mod external_io;
pub mod main_batch_executor;
pub mod mempool_io;
pub mod output_handler;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::{
            BatchExecutorResource, ConditionalSealerResource, OutputHandlerResource,
            StateKeeperIOResource,
        },
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Requests:
/// - `StateKeeperIOResource`
/// - `BatchExecutorResource`
/// - `ConditionalSealerResource`
///
#[derive(Debug)]
pub struct StateKeeperLayer {
    state_keeper_db_path: String,
    state_keeper_db_block_cache_capacity: usize,
    state_keeper_db_max_open_files: Option<NonZero<u32>>,
}

impl StateKeeperLayer {
    pub fn new(
        state_keeper_db_path: String,
        state_keeper_db_block_cache_capacity: usize,
        state_keeper_db_max_open_files: Option<NonZero<u32>>,
    ) -> Self {
        Self {
            state_keeper_db_path,
            state_keeper_db_block_cache_capacity,
            state_keeper_db_max_open_files,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for StateKeeperLayer {
    fn layer_name(&self) -> &'static str {
        "state_keeper_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let io = context
            .get_resource::<StateKeeperIOResource>()
            .await?
            .0
            .take()
            .context("StateKeeperIO was provided but taken by some other task")?;
        let batch_executor_base = context
            .get_resource::<BatchExecutorResource>()
            .await?
            .0
            .take()
            .context("L1BatchExecutorBuilder was provided but taken by some other task")?;
        let output_handler = context
            .get_resource::<OutputHandlerResource>()
            .await?
            .0
            .take()
            .context("HandleStateKeeperOutput was provided but taken by another task")?;
        let sealer = context.get_resource::<ConditionalSealerResource>().await?.0;
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;

        let cache_options = RocksdbStorageOptions {
            block_cache_capacity: self.state_keeper_db_block_cache_capacity,
            max_open_files: self.state_keeper_db_max_open_files,
        };
        let (storage_factory, task) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            cache_options,
        );
        context.add_task(Box::new(RocksdbCatchupTask(task)));

        context.add_task(Box::new(StateKeeperTask {
            io,
            batch_executor_base,
            output_handler,
            sealer,
            storage_factory: Arc::new(storage_factory),
        }));
        Ok(())
    }
}

#[derive(Debug)]
struct StateKeeperTask {
    io: Box<dyn StateKeeperIO>,
    batch_executor_base: Box<dyn BatchExecutor>,
    output_handler: OutputHandler,
    sealer: Arc<dyn ConditionalSealer>,
    storage_factory: Arc<dyn ReadStorageFactory>,
}

#[async_trait::async_trait]
impl Task for StateKeeperTask {
    fn id(&self) -> TaskId {
        "state_keeper".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let state_keeper = ZkSyncStateKeeper::new(
            stop_receiver.0,
            self.io,
            self.batch_executor_base,
            self.output_handler,
            self.sealer,
            self.storage_factory,
        );
        let result = state_keeper.run().await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .unwrap();

        result
    }
}

#[derive(Debug)]
struct RocksdbCatchupTask(AsyncCatchupTask);

#[async_trait::async_trait]
impl Task for RocksdbCatchupTask {
    fn id(&self) -> TaskId {
        "state_keeper/rocksdb_catchup_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0.clone()).await?;
        stop_receiver.0.changed().await?;
        Ok(())
    }
}

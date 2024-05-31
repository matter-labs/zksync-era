use std::sync::Arc;

use anyhow::Context;
use zksync_config::DBConfig;
use zksync_state::{AsyncCatchupTask, ReadStorageFactory, RocksdbStorageOptions};
use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, AsyncRocksdbCache, BatchExecutor, OutputHandler,
    StateKeeperIO, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;

pub mod main_batch_executor;
pub mod mempool_io;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::{
            BatchExecutorResource, ConditionalSealerResource, OutputHandlerResource,
            StateKeeperIOResource,
        },
    },
    service::{Provides, ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug, Provides)]
#[provides(local = "true", StateKeeperTask, RocksdbCatchupTask)]
pub struct StateKeeperLayer {
    db_config: DBConfig,
}

impl StateKeeperLayer {
    pub fn new(db_config: DBConfig) -> Self {
        Self { db_config }
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
            block_cache_capacity: self
                .db_config
                .experimental
                .state_keeper_db_block_cache_capacity(),
            max_open_files: self.db_config.experimental.state_keeper_db_max_open_files,
        };
        let (storage_factory, task) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.db_config.state_keeper_db_path.clone(),
            cache_options,
        );
        context.add_task(context.token::<Self>(), Box::new(RocksdbCatchupTask(task)));

        context.add_task(
            context.token::<Self>(),
            Box::new(StateKeeperTask {
                io,
                batch_executor_base,
                output_handler,
                sealer,
                storage_factory: Arc::new(storage_factory),
            }),
        );
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
    fn name(&self) -> &'static str {
        "state_keeper"
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
    fn name(&self) -> &'static str {
        "state_keeper/rocksdb_catchup_task"
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0.clone()).await?;
        stop_receiver.0.changed().await?;
        Ok(())
    }
}

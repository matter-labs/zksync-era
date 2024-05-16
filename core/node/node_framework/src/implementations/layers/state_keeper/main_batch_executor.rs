use std::sync::Arc;

use zksync_config::{configs::chain::StateKeeperConfig, DBConfig};
use zksync_state::{AsyncCatchupTask, RocksdbStorageOptions};
use zksync_state_keeper::{AsyncRocksdbCache, MainBatchExecutor};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::BatchExecutorResource,
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    db_config: DBConfig,
    state_keeper_config: StateKeeperConfig,
}

impl MainBatchExecutorLayer {
    pub fn new(db_config: DBConfig, state_keeper_config: StateKeeperConfig) -> Self {
        Self {
            db_config,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainBatchExecutorLayer {
    fn layer_name(&self) -> &'static str {
        "main_batch_executor_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;

        let cache_options = RocksdbStorageOptions {
            block_cache_capacity: self
                .db_config
                .experimental
                .state_keeper_db_block_cache_capacity(),
            max_open_files: self.db_config.experimental.state_keeper_db_max_open_files,
        };
        let (storage_factory, task) = AsyncRocksdbCache::new(
            master_pool.get_singleton().await?,
            self.db_config.state_keeper_db_path,
            cache_options,
        );
        let builder = MainBatchExecutor::new(
            Arc::new(storage_factory),
            self.state_keeper_config.save_call_traces,
            false,
        );

        context.insert_resource(BatchExecutorResource(Unique::new(Box::new(builder))))?;
        context.add_task(Box::new(RocksdbCatchupTask(task)));
        Ok(())
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

use std::sync::Arc;

use anyhow::Context;
use zksync_state::{AsyncCatchupTask, ReadStorageFactory};
use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, AsyncRocksdbCache, BatchExecutor, OutputHandler,
    StateKeeperIO, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;

pub mod external_io;
pub mod main_batch_executor;
pub mod mempool_io;
pub mod output_handler;

// Public re-export to not require the user to directly depend on `zksync_state`.
pub use zksync_state::RocksdbStorageOptions;

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

/// Wiring layer for the state keeper.
///
/// ## Requests resources
///
/// - `StateKeeperIOResource`
/// - `BatchExecutorResource`
/// - `OutputHandlerResource`
/// - `ConditionalSealerResource`
/// - `PoolResource<MasterPool>`
///
/// ## Adds tasks
///
/// - `RocksdbCatchupTask`
/// - `StateKeeperTask`
#[derive(Debug)]
pub struct StateKeeperLayer {
    state_keeper_db_path: String,
    rocksdb_options: RocksdbStorageOptions,
}

impl StateKeeperLayer {
    pub fn new(state_keeper_db_path: String, rocksdb_options: RocksdbStorageOptions) -> Self {
        Self {
            state_keeper_db_path,
            rocksdb_options,
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
            .get_resource::<StateKeeperIOResource>()?
            .0
            .take()
            .context("StateKeeperIO was provided but taken by some other task")?;
        let batch_executor_base = context
            .get_resource::<BatchExecutorResource>()?
            .0
            .take()
            .context("L1BatchExecutorBuilder was provided but taken by some other task")?;
        let output_handler = context
            .get_resource::<OutputHandlerResource>()?
            .0
            .take()
            .context("HandleStateKeeperOutput was provided but taken by another task")?;
        let sealer = context.get_resource::<ConditionalSealerResource>()?.0;
        let master_pool = context.get_resource::<PoolResource<MasterPool>>()?;

        let (storage_factory, task) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            self.rocksdb_options,
        );
        context.add_task(Box::new(RocksdbCatchupTask(task)));

        context.add_task(Box::new(StateKeeperTask {
            io,
            batch_executor_base,
            output_handler,
            sealer,
            storage_factory: Arc::new(storage_factory),
        }));

        context.add_shutdown_hook("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });
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
        state_keeper.run().await
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

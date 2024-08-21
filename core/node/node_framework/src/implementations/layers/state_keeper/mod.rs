use std::sync::Arc;

use anyhow::Context;
use zksync_state::{AsyncCatchupTask, OwnedStorage, ReadStorageFactory};
use zksync_state_keeper::{
    executor::MainStateKeeperExecutorFactory, seal_criteria::ConditionalSealer, AsyncRocksdbCache,
    OutputHandler, StateKeeperIO, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;

pub mod external_io;
pub mod main_batch_executor;
pub mod mempool_io;
pub mod output_handler;

// Public re-export to not require the user to directly depend on `zksync_state`.
pub use zksync_state::RocksdbStorageOptions;
use zksync_vm_utils::interface::BoxBatchExecutorFactory;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::{
            BatchExecutorResource, ConditionalSealerResource, OutputHandlerResource,
            StateKeeperIOResource,
        },
    },
    service::{ShutdownHook, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for the state keeper.
#[derive(Debug)]
pub struct StateKeeperLayer {
    state_keeper_db_path: String,
    rocksdb_options: RocksdbStorageOptions,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub state_keeper_io: StateKeeperIOResource,
    pub batch_executor: BatchExecutorResource,
    pub output_handler: OutputHandlerResource,
    pub conditional_sealer: ConditionalSealerResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub state_keeper: StateKeeperTask,
    #[context(task)]
    pub rocksdb_catchup: AsyncCatchupTask,
    pub rocksdb_termination_hook: ShutdownHook,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "state_keeper_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let io = input
            .state_keeper_io
            .0
            .take()
            .context("StateKeeperIO was provided but taken by some other task")?;
        let batch_executor_base = input
            .batch_executor
            .0
            .take()
            .context("L1BatchExecutorBuilder was provided but taken by some other task")?;
        let output_handler = input
            .output_handler
            .0
            .take()
            .context("HandleStateKeeperOutput was provided but taken by another task")?;
        let sealer = input.conditional_sealer.0;
        let master_pool = input.master_pool;

        let (storage_factory, rocksdb_catchup) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            self.rocksdb_options,
        );

        let state_keeper = StateKeeperTask {
            io,
            batch_executor: batch_executor_base,
            output_handler,
            sealer,
            storage_factory: Arc::new(storage_factory),
        };

        let rocksdb_termination_hook = ShutdownHook::new("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });
        Ok(Output {
            state_keeper,
            rocksdb_catchup,
            rocksdb_termination_hook,
        })
    }
}

#[derive(Debug)]
pub struct StateKeeperTask {
    io: Box<dyn StateKeeperIO>,
    batch_executor: BoxBatchExecutorFactory<OwnedStorage>,
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
        let executor =
            MainStateKeeperExecutorFactory::custom(self.batch_executor, self.storage_factory);
        let state_keeper = ZkSyncStateKeeper::new(
            stop_receiver.0,
            self.io,
            Box::new(executor),
            self.output_handler,
            self.sealer,
        );
        state_keeper.run().await
    }
}

#[async_trait::async_trait]
impl Task for AsyncCatchupTask {
    fn kind(&self) -> TaskKind {
        TaskKind::OneshotTask
    }

    fn id(&self) -> TaskId {
        "state_keeper/rocksdb_catchup_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

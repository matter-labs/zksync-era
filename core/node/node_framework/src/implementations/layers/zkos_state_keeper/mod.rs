use std::sync::Arc;

use anyhow::Context;
use zksync_node_framework_derive::{FromContext, IntoContext};
use zksync_state::{AsyncCatchupTask, KeepUpdatedTask, RocksdbStorageOptions};
use zksync_storage::RocksDB;
use zksync_zkos_state_keeper::{
    state_keeper_storage::ZkOsAsyncRocksdbCache, ConditionalSealer, OutputHandler, StateKeeperIO,
    ZkosStateKeeper,
};

use crate::{
    implementations::resources::{
        fee_input::SequencerFeeInputResource,
        pools::{MasterPool, PoolResource},
        state_keeper::{
            ZkOsConditionalSealerResource, ZkOsOutputHandlerResource, ZkOsStateKeeperIOResource,
        },
    },
    service::ShutdownHook,
    StopReceiver, Task, TaskId, WiringError, WiringLayer,
};

pub mod mempool_io;
pub mod output_handler;

/// Wiring layer for the state keeper.
#[derive(Debug)]
pub struct ZkOsStateKeeperLayer {
    state_keeper_db_path: String,
    rocksdb_options: RocksdbStorageOptions,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub state_keeper_io: ZkOsStateKeeperIOResource,
    pub output_handler: ZkOsOutputHandlerResource,
    pub fee_input: SequencerFeeInputResource,
    pub conditional_sealer: ZkOsConditionalSealerResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub state_keeper: ZkOsStateKeeperTask,
    #[context(task)]
    pub rocksdb_catchup: AsyncCatchupTask,
    #[context(task)]
    pub rocksdb_keep_updated: KeepUpdatedTask,
    pub rocksdb_termination_hook: ShutdownHook,
}

impl ZkOsStateKeeperLayer {
    pub fn new(state_keeper_db_path: String, rocksdb_options: RocksdbStorageOptions) -> Self {
        Self {
            state_keeper_db_path,
            rocksdb_options,
        }
    }
}

#[derive(Debug)]
pub struct ZkOsStateKeeperTask {
    pool: PoolResource<MasterPool>,
    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
    storage_factory: Arc<ZkOsAsyncRocksdbCache>,
    sealer: Arc<dyn ConditionalSealer>,
}

#[async_trait::async_trait]
impl WiringLayer for ZkOsStateKeeperLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "zk_os_state_keeper_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool;

        let io = input
            .state_keeper_io
            .0
            .take()
            .context("StateKeeperIO was provided but taken by some other task")?;

        let output_handler = input
            .output_handler
            .0
            .take()
            .context("HandleStateKeeperOutput was provided but taken by another task")?;

        let (storage_factory, rocksdb_catchup, rocksdb_keep_updated) = ZkOsAsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            self.rocksdb_options,
        );

        let state_keeper = ZkOsStateKeeperTask {
            pool: master_pool.clone(),
            io,
            output_handler,
            storage_factory: Arc::new(storage_factory),
            sealer: input.conditional_sealer.0,
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
            rocksdb_keep_updated,
            rocksdb_termination_hook,
        })
    }
}

#[async_trait::async_trait]
impl Task for ZkOsStateKeeperTask {
    fn id(&self) -> TaskId {
        "state_keeper".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let state_keeper = ZkosStateKeeper::new(
            stop_receiver.0,
            self.pool.get().await?,
            self.io,
            self.output_handler,
            self.storage_factory,
            self.sealer,
        );
        state_keeper.run().await
    }
}

#[async_trait::async_trait]
impl Task for KeepUpdatedTask {
    fn id(&self) -> TaskId {
        "state_keeper/rocksdb_keep_updated_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

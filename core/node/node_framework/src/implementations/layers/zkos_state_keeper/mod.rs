use std::sync::Arc;

use anyhow::Context;
use zksync_config::configs::chain::MempoolConfig;
use zksync_node_framework_derive::{FromContext, IntoContext};
use zksync_state::{
    AsyncCatchupTask, CommonStorage::Postgres, OwnedStorage, ReadStorageFactory,
    RocksdbStorageOptions,
};
use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, AsyncRocksdbCache, MempoolFetcher, MempoolGuard,
    ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;
use zksync_vm_executor::interface::BatchExecutorFactory;
use zksync_zkos_state_keeper::{OutputHandler, StateKeeperIO, ZkosStateKeeper};

use crate::{
    implementations::resources::{
        fee_input::SequencerFeeInputResource,
        pools::{MasterPool, PoolResource},
        state_keeper::{ZkOsOutputHandlerResource, ZkOsStateKeeperIOResource},
    },
    service::ShutdownHook,
    StopReceiver, Task, TaskId, WiringError, WiringLayer,
};

pub mod mempool_io;
pub mod output_handler;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub state_keeper_io: ZkOsStateKeeperIOResource,
    pub output_handler: ZkOsOutputHandlerResource,
    pub fee_input: SequencerFeeInputResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub state_keeper: ZkOsStateKeeperTask,
}

#[derive(Debug)]
pub struct ZkOsStateKeeperLayer {
    mempool_config: MempoolConfig,
}

impl ZkOsStateKeeperLayer {
    pub fn new(mempool_config: MempoolConfig) -> Self {
        Self { mempool_config }
    }
}

#[derive(Debug)]
pub struct ZkOsStateKeeperTask {
    pool: PoolResource<MasterPool>,
    io: Box<dyn StateKeeperIO>,
    output_handler: OutputHandler,
}

impl ZkOsStateKeeperLayer {
    async fn build_mempool_guard(
        &self,
        master_pool: &PoolResource<MasterPool>,
    ) -> anyhow::Result<MempoolGuard> {
        let connection_pool = master_pool
            .get_singleton()
            .await
            .context("Get connection pool")?;
        let mut storage = connection_pool
            .connection()
            .await
            .context("Access storage to build mempool")?;
        let mempool = MempoolGuard::from_storage(&mut storage, self.mempool_config.capacity).await;
        mempool.register_metrics();
        Ok(mempool)
    }
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

        let state_keeper = ZkOsStateKeeperTask {
            pool: master_pool.clone(),
            io,
            output_handler,
        };

        Ok(Output { state_keeper })
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
        );
        state_keeper.run().await
    }
}

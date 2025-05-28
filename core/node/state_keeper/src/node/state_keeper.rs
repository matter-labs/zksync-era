use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use zksync_dal::node::{MasterPool, PoolResource, ReplicaPool};
use zksync_health_check::{node::AppHealthCheckResource, ReactiveHealthCheck};
use zksync_node_framework::{
    service::ShutdownHook, task::TaskKind, FromContext, IntoContext, StopReceiver, Task, TaskId,
    WiringError, WiringLayer,
};
use zksync_state::{AsyncCatchupTask, RocksdbStorageOptions};
use zksync_storage::RocksDB;
use zksync_types::try_stoppable;
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};

use super::resources::{
    BatchExecutorResource, ConditionalSealerResource, OutputHandlerResource, StateKeeperIOResource,
};
use crate::{AsyncRocksdbCache, StateKeeperInner};

/// Wiring layer for the state keeper.
#[derive(Debug)]
pub struct StateKeeperLayer {
    state_keeper_db_path: PathBuf,
    rocksdb_options: RocksdbStorageOptions,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub state_keeper_io: StateKeeperIOResource,
    pub batch_executor: BatchExecutorResource,
    pub output_handler: OutputHandlerResource,
    pub conditional_sealer: ConditionalSealerResource,
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub shared_allow_list: Option<SharedAllowList>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    state_keeper: StateKeeperTask,
    #[context(task)]
    rocksdb_catchup: AsyncCatchupTaskWrapper,
    rocksdb_termination_hook: ShutdownHook,
}

impl StateKeeperLayer {
    pub fn new(state_keeper_db_path: PathBuf, rocksdb_options: RocksdbStorageOptions) -> Self {
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

        let (storage_factory, mut rocksdb_catchup) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            self.rocksdb_options,
        );
        // The number of connections in the recovery DB pool (~recovery concurrency) is based on the Era mainnet recovery runs.
        let recovery_pool = input.replica_pool.get_custom(10).await?;
        rocksdb_catchup = rocksdb_catchup.with_recovery_pool(recovery_pool);

        let state_keeper_inner = StateKeeperInner::new(
            io,
            batch_executor_base,
            output_handler,
            sealer,
            Arc::new(storage_factory),
            input.shared_allow_list.map(DeploymentTxFilter::new),
        );

        let state_keeper = StateKeeperTask { state_keeper_inner };

        input
            .app_health
            .0
            .insert_component(state_keeper.health_check())
            .map_err(WiringError::internal)?;

        let rocksdb_termination_hook = ShutdownHook::new("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });
        Ok(Output {
            state_keeper,
            rocksdb_catchup: AsyncCatchupTaskWrapper(rocksdb_catchup),
            rocksdb_termination_hook,
        })
    }
}

#[derive(Debug)]
pub struct StateKeeperTask {
    state_keeper_inner: StateKeeperInner,
}

impl StateKeeperTask {
    /// Returns the health check for state keeper.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.state_keeper_inner.health_check()
    }
}

#[async_trait::async_trait]
impl Task for StateKeeperTask {
    fn id(&self) -> TaskId {
        "state_keeper".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let state_keeper =
            try_stoppable!(self.state_keeper_inner.initialize(&stop_receiver.0).await);
        state_keeper.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct AsyncCatchupTaskWrapper(AsyncCatchupTask);

#[async_trait::async_trait]
impl Task for AsyncCatchupTaskWrapper {
    fn kind(&self) -> TaskKind {
        TaskKind::OneshotTask
    }

    fn id(&self) -> TaskId {
        "state_keeper/rocksdb_catchup_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}

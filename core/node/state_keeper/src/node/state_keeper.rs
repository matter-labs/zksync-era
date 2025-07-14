use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use zksync_dal::node::{MasterPool, PoolResource, ReplicaPool};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::ShutdownHook, task::TaskKind, FromContext, IntoContext, StopReceiver, Task, TaskId,
    WiringError, WiringLayer,
};
use zksync_state::{AsyncCatchupTask, RocksdbStorageOptions};
use zksync_storage::RocksDB;
use zksync_types::try_stoppable;
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};
use zksync_eth_client::web3_decl::node::SettlementModeResource;

use super::resources::{BatchExecutorResource, OutputHandlerResource, StateKeeperIOResource};
use crate::{seal_criteria::ConditionalSealer, AsyncRocksdbCache, StateKeeperBuilder};

/// Wiring layer for the state keeper.
#[derive(Debug)]
pub struct StateKeeperLayer {
    state_keeper_db_path: PathBuf,
    rocksdb_options: RocksdbStorageOptions,
}

#[derive(Debug, FromContext)]
pub struct Input {
    state_keeper_io: StateKeeperIOResource,
    batch_executor: BatchExecutorResource,
    output_handler: OutputHandlerResource,
    conditional_sealer: Arc<dyn ConditionalSealer>,
    master_pool: PoolResource<MasterPool>,
    replica_pool: PoolResource<ReplicaPool>,
    shared_allow_list: Option<SharedAllowList>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
    settlement_mode: SettlementModeResource,
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
        let sealer = input.conditional_sealer;
        let master_pool = input.master_pool;

        let (storage_factory, mut rocksdb_catchup) = AsyncRocksdbCache::new(
            master_pool.get_custom(2).await?,
            self.state_keeper_db_path,
            self.rocksdb_options,
        );
        // The number of connections in the recovery DB pool (~recovery concurrency) is based on the Era mainnet recovery runs.
        let recovery_pool = input.replica_pool.get_custom(10).await?;
        rocksdb_catchup = rocksdb_catchup.with_recovery_pool(recovery_pool);

        let state_keeper_builder = StateKeeperBuilder::new(
            io,
            batch_executor_base,
            output_handler,
            sealer,
            Arc::new(storage_factory),
            input.shared_allow_list.map(DeploymentTxFilter::new),
        );

        input
            .app_health
            .insert_component(state_keeper_builder.health_check())
            .map_err(WiringError::internal)?;
        let state_keeper = StateKeeperTask {
            state_keeper_builder,
        };

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
    state_keeper_builder: StateKeeperBuilder,
}

#[async_trait::async_trait]
impl Task for StateKeeperTask {
    fn id(&self) -> TaskId {
        "state_keeper".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let state_keeper = try_stoppable!(self.state_keeper_builder.build(&stop_receiver.0).await);
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

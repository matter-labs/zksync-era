use std::sync::Arc;
use anyhow::Context;
use zksync_config::configs::chain::MempoolConfig;
use zksync_node_framework_derive::{FromContext, IntoContext};
use zksync_state::{AsyncCatchupTask, OwnedStorage, ReadStorageFactory, RocksdbStorageOptions};
use zksync_state::CommonStorage::Postgres;
use zksync_state_keeper::{AsyncRocksdbCache, MempoolFetcher, MempoolGuard, OutputHandler, StateKeeperIO, ZkSyncStateKeeper};
use zksync_state_keeper::seal_criteria::ConditionalSealer;
use zksync_storage::RocksDB;
use zksync_vm_executor::interface::BatchExecutorFactory;
use zksync_zkos_state_keeper::ZkosStateKeeper;
use crate::implementations::resources::pools::{MasterPool, PoolResource};
use crate::service::ShutdownHook;
use crate::{StopReceiver, Task, TaskId, WiringError, WiringLayer};
use crate::implementations::resources::fee_input::SequencerFeeInputResource;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub fee_input: SequencerFeeInputResource,
    pub master_pool: PoolResource<MasterPool>,
}


#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub state_keeper: ZkOsStateKeeperTask,
    // todo: add rocksDB cache
    #[context(task)]
    pub mempool_fetcher: MempoolFetcher,
}

#[derive(Debug)]
pub struct ZkOsStateKeeperLayer {
    // rocksdb_options: RocksdbStorageOptions,
    mempool_config: MempoolConfig
}

impl ZkOsStateKeeperLayer {
    pub fn new(mempool_config: MempoolConfig) -> Self {
        Self {
            // rocksdb_options,
            mempool_config
        }
    }
}

#[derive(Debug)]
pub struct ZkOsStateKeeperTask {
    pool: PoolResource<MasterPool>,
    // storage_factory: Arc<dyn ReadStorageFactory>,
    mempool_guard: MempoolGuard
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

        let mempool_guard = self.build_mempool_guard(&master_pool).await?;
        // let (storage_factory, rocksdb_catchup) = Postgres()
        //
        //     AsyncRocksdbCache::new(
        //     master_pool.get_custom(2).await?,
        //     self.state_keeper_db_path,
        //     self.rocksdb_options,
        // );


        let rocksdb_termination_hook = ShutdownHook::new("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });

        let mempool_fetcher_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let batch_fee_input_provider = input.fee_input.0;

        let mempool_fetcher = MempoolFetcher::new(
            mempool_guard.clone(),
            batch_fee_input_provider.clone(),
            &self.mempool_config,
            mempool_fetcher_pool,
        );

        let state_keeper = ZkOsStateKeeperTask {
            // storage_factory: Arc::new(storage_factory),
            pool: master_pool.clone(),
            mempool_guard
        };

        Ok(Output {
            state_keeper,
            // rocksdb_termination_hook,
            mempool_fetcher
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
            // self.storage_factory.clone(),
            self.mempool_guard,
        );
        state_keeper.run().await
    }
}

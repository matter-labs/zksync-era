use std::sync::Arc;

use anyhow::Context as _;
use zksync_config::{
    configs::{
        chain::{MempoolConfig, StateKeeperConfig},
        wallets,
    },
    ContractsConfig,
};
use zksync_state_keeper::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, MempoolFetcher, MempoolGuard,
    MempoolIO, OutputHandler, SequencerSealer, StateKeeperPersistence, TreeWritesPersistence,
};
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource,
        pools::{MasterPool, PoolResource},
        state_keeper::{ConditionalSealerResource, OutputHandlerResource, StateKeeperIOResource},
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MempoolIOLayer {
    zksync_network_id: L2ChainId,
    contracts_config: ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
    wallets: wallets::StateKeeper,
}

impl MempoolIOLayer {
    pub fn new(
        zksync_network_id: L2ChainId,
        contracts_config: ContractsConfig,
        state_keeper_config: StateKeeperConfig,
        mempool_config: MempoolConfig,
        wallets: wallets::StateKeeper,
    ) -> Self {
        Self {
            zksync_network_id,
            contracts_config,
            state_keeper_config,
            mempool_config,
            wallets,
        }
    }

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
impl WiringLayer for MempoolIOLayer {
    fn layer_name(&self) -> &'static str {
        "mempool_io_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Fetch required resources.
        let batch_fee_input_provider = context.get_resource::<FeeInputResource>().await?.0;
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;

        // Create L2 block sealer task and output handler.
        // L2 Block sealing process is parallelized, so we have to provide enough pooled connections.
        let persistence_pool = master_pool
            .get_custom(L2BlockSealProcess::subtasks_len())
            .await
            .context("Get master pool")?;
        let (persistence, l2_block_sealer) = StateKeeperPersistence::new(
            persistence_pool.clone(),
            self.contracts_config.l2_shared_bridge_addr.unwrap(),
            self.contracts_config
                .l2_native_token_vault_proxy_addr
                .unwrap(),
            self.state_keeper_config.l2_block_seal_queue_capacity,
        );
        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
        context.insert_resource(OutputHandlerResource(Unique::new(output_handler)))?;
        context.add_task(Box::new(L2BlockSealerTask(l2_block_sealer)));

        // Create mempool fetcher task.
        let mempool_guard = self.build_mempool_guard(&master_pool).await?;
        let mempool_fetcher_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let mempool_fetcher = MempoolFetcher::new(
            mempool_guard.clone(),
            batch_fee_input_provider.clone(),
            &self.mempool_config,
            mempool_fetcher_pool,
        );
        context.add_task(Box::new(MempoolFetcherTask(mempool_fetcher)));

        // Create mempool IO resource.
        let mempool_db_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let io = MempoolIO::new(
            mempool_guard,
            batch_fee_input_provider,
            mempool_db_pool,
            &self.state_keeper_config,
            self.wallets.fee_account.address(),
            self.mempool_config.delay_interval(),
            self.zksync_network_id,
        )
        .await?;
        context.insert_resource(StateKeeperIOResource(Unique::new(Box::new(io))))?;

        // Create sealer.
        let sealer = SequencerSealer::new(self.state_keeper_config);
        context.insert_resource(ConditionalSealerResource(Arc::new(sealer)))?;

        Ok(())
    }
}

#[derive(Debug)]
struct L2BlockSealerTask(zksync_state_keeper::L2BlockSealerTask);

#[async_trait::async_trait]
impl Task for L2BlockSealerTask {
    fn id(&self) -> TaskId {
        "state_keeper/l2_block_sealer".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Miniblock sealer will exit itself once sender is dropped.
        self.0.run().await
    }
}

#[derive(Debug)]
struct MempoolFetcherTask(MempoolFetcher);

#[async_trait::async_trait]
impl Task for MempoolFetcherTask {
    fn id(&self) -> TaskId {
        "state_keeper/mempool_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}

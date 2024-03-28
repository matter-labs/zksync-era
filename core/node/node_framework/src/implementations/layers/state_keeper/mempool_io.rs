use std::sync::Arc;

use anyhow::Context as _;
use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, StateKeeperConfig},
    ContractsConfig,
};
use zksync_core::state_keeper::{
    self, MempoolFetcher, MempoolGuard, MempoolIO, OutputHandler, SequencerSealer,
    StateKeeperPersistence,
};

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource,
        pools::MasterPoolResource,
        state_keeper::{ConditionalSealerResource, OutputHandlerResource, StateKeeperIOResource},
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MempoolIOLayer {
    network_config: NetworkConfig,
    contracts_config: ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
}

impl MempoolIOLayer {
    pub fn new(
        network_config: NetworkConfig,
        contracts_config: ContractsConfig,
        state_keeper_config: StateKeeperConfig,
        mempool_config: MempoolConfig,
    ) -> Self {
        Self {
            network_config,
            contracts_config,
            state_keeper_config,
            mempool_config,
        }
    }

    async fn build_mempool_guard(
        &self,
        master_pool: &MasterPoolResource,
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
        let master_pool = context.get_resource::<MasterPoolResource>().await?;

        // Create miniblock sealer task.
        let (persistence, miniblock_sealer) = StateKeeperPersistence::new(
            master_pool
                .get_singleton()
                .await
                .context("Get master pool")?,
            self.contracts_config.l2_erc20_bridge_addr,
            self.state_keeper_config.miniblock_seal_queue_capacity,
        );
        let output_handler = OutputHandler::new(Box::new(persistence));
        context.insert_resource(OutputHandlerResource(Unique::new(output_handler)))?;
        context.add_task(Box::new(MiniblockSealerTask(miniblock_sealer)));

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
            self.mempool_config.delay_interval(),
            self.network_config.zksync_network_id,
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
struct MiniblockSealerTask(state_keeper::MiniblockSealerTask);

#[async_trait::async_trait]
impl Task for MiniblockSealerTask {
    fn name(&self) -> &'static str {
        "state_keeper/miniblock_sealer"
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
    fn name(&self) -> &'static str {
        "state_keeper/mempool_fetcher"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}

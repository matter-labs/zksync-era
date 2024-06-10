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
        sync_state::SyncStateResource,
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ExternalIOLayer {
    zksync_network_id: L2ChainId,
    contracts_config: ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
    wallets: wallets::StateKeeper,
}

impl ExternalIOLayer {
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
}

#[async_trait::async_trait]
impl WiringLayer for ExternalIOLayer {
    fn layer_name(&self) -> &'static str {
        "external_io_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Fetch required resources.
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
            self.state_keeper_config.l2_block_seal_queue_capacity,
        );
        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let mut output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
        context.insert_resource(OutputHandlerResource(Unique::new(output_handler)))?;

        // Create mempool IO resource.
        let mempool_db_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let io = todo!();

        // context.insert_resource(StateKeeperIOResource(Unique::new(Box::new(io))))?;

        // // Create sealer.
        // let sealer = SequencerSealer::new(self.state_keeper_config);
        // context.insert_resource(ConditionalSealerResource(Arc::new(sealer)))?;

        Ok(())
    }
}

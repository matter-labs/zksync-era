use std::sync::Arc;

use anyhow::Context as _;
use zksync_node_sync::{ActionQueue, ExternalIO, SyncState};
use zksync_state_keeper::seal_criteria::NoopSealer;
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        action_queue::ActionQueueSenderResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        state_keeper::{ConditionalSealerResource, StateKeeperIOResource},
        sync_state::SyncStateResource,
    },
    resource::Unique,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ExternalIOLayer {
    chain_id: L2ChainId,
}

impl ExternalIOLayer {
    pub fn new(chain_id: L2ChainId) -> Self {
        Self { chain_id }
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
        let MainNodeClientResource(main_node_client) = context.get_resource().await?;

        // Create `SyncState` resource.
        let sync_state = SyncState::default();
        context.insert_resource(SyncStateResource(sync_state))?;

        // Create `ActionQueueSender` resource.
        let (action_queue_sender, action_queue) = ActionQueue::new();
        context.insert_resource(ActionQueueSenderResource(Unique::new(action_queue_sender)))?;

        // Create external IO resource.
        let io_pool = master_pool.get().await.context("Get master pool")?;
        let io = ExternalIO::new(
            io_pool,
            action_queue,
            Box::new(main_node_client.for_component("external_io")),
            self.chain_id,
        )
        .await
        .context("Failed initializing I/O for external node state keeper")?;
        context.insert_resource(StateKeeperIOResource(Unique::new(Box::new(io))))?;

        // Create sealer.
        context.insert_resource(ConditionalSealerResource(Arc::new(NoopSealer)))?;

        Ok(())
    }
}

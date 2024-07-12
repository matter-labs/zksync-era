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
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `ExternalIO`, an IO part of state keeper used by the external node.
#[derive(Debug)]
pub struct ExternalIOLayer {
    chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub sync_state: SyncStateResource,
    pub action_queue_sender: ActionQueueSenderResource,
    pub io: StateKeeperIOResource,
    pub sealer: ConditionalSealerResource,
}

impl ExternalIOLayer {
    pub fn new(chain_id: L2ChainId) -> Self {
        Self { chain_id }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ExternalIOLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_io_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Create `SyncState` resource.
        let sync_state = SyncState::default();

        // Create `ActionQueueSender` resource.
        let (action_queue_sender, action_queue) = ActionQueue::new();

        // Create external IO resource.
        let io_pool = input.pool.get().await.context("Get master pool")?;
        let io = ExternalIO::new(
            io_pool,
            action_queue,
            Box::new(input.main_node_client.0.for_component("external_io")),
            self.chain_id,
        )
        .context("Failed initializing I/O for external node state keeper")?;

        // Create sealer.
        let sealer = ConditionalSealerResource(Arc::new(NoopSealer));

        Ok(Output {
            sync_state: sync_state.into(),
            action_queue_sender: action_queue_sender.into(),
            io: io.into(),
            sealer,
        })
    }
}

use std::sync::Arc;

use anyhow::Context as _;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::{api::SyncState, contracts::L1ChainContractsResource};
use zksync_state_keeper::{
    node::StateKeeperIOResource,
    seal_criteria::{ConditionalSealer, NoopSealer},
};
use zksync_types::L2ChainId;
use zksync_web3_decl::client::{DynClient, L1, L2};

use super::resources::ActionQueueSenderResource;
use crate::{external_io::PriorityTxVerifierL1, ActionQueue, ExternalIO};

/// Wiring layer for `ExternalIO`, an IO part of state keeper used by the external node.
#[derive(Debug)]
pub struct ExternalIOLayer {
    chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    app_health: Arc<AppHealthCheck>,
    pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
    l1_node: Box<DynClient<L1>>,
    l1_chain_contracts_resource: L1ChainContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    sync_state: SyncState,
    action_queue_sender: ActionQueueSenderResource,
    io: StateKeeperIOResource,
    sealer: Arc<dyn ConditionalSealer>,
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
        input
            .app_health
            .insert_custom_component(Arc::new(sync_state.clone()))
            .map_err(WiringError::internal)?;

        // Create `ActionQueueSender` resource.
        let (action_queue_sender, action_queue) = ActionQueue::new();

        let priority_tx_verifier = PriorityTxVerifierL1::new(
            input.l1_node.for_component("priority_tx_verifier"),
            input
                .l1_chain_contracts_resource
                .0
                .chain_contracts_config
                .diamond_proxy_addr,
        );

        // Create external IO resource.
        let io_pool = input.pool.get().await.context("Get master pool")?;
        let io = ExternalIO::new(
            io_pool,
            action_queue,
            Box::new(input.main_node_client.for_component("external_io")),
            self.chain_id,
            Box::new(priority_tx_verifier),
        )
        .context("Failed initializing I/O for external node state keeper")?;

        // Create sealer.
        let sealer = Arc::new(NoopSealer);

        Ok(Output {
            sync_state,
            action_queue_sender: action_queue_sender.into(),
            io: io.into(),
            sealer,
        })
    }
}

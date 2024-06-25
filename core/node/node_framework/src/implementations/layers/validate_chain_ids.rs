use zksync_node_sync::validate_chain_ids_task::ValidateChainIdsTask;
use zksync_types::{L1ChainId, L2ChainId};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, main_node_client::MainNodeClientResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for chain ID validation precondition for external node.
/// Ensures that chain IDs are consistent locally, on main node, and on L1.
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
/// - `MainNodeClientResource
///
/// ## Adds preconditions
///
/// - `ValidateChainIdsTask`
#[derive(Debug)]
pub struct ValidateChainIdsLayer {
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
}

impl ValidateChainIdsLayer {
    pub fn new(l1_chain_id: L1ChainId, l2_chain_id: L2ChainId) -> Self {
        Self {
            l1_chain_id,
            l2_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ValidateChainIdsLayer {
    fn layer_name(&self) -> &'static str {
        "validate_chain_ids_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let EthInterfaceResource(query_client) = context.get_resource().await?;
        let MainNodeClientResource(main_node_client) = context.get_resource().await?;

        let task = ValidateChainIdsTask::new(
            self.l1_chain_id,
            self.l2_chain_id,
            query_client,
            main_node_client,
        );

        context.add_task(Box::new(task));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for ValidateChainIdsTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Precondition
    }

    fn id(&self) -> TaskId {
        "validate_chain_ids".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run_once(stop_receiver.0).await
    }
}

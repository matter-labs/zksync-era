use zksync_node_sync::validate_chain_ids_task::ValidateChainIdsTask;
use zksync_types::{L1ChainId, L2ChainId};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, main_node_client::MainNodeClientResource,
    },
    precondition::Precondition,
    service::{ServiceContext, StopReceiver},
    task::TaskId,
    wiring_layer::{WiringError, WiringLayer},
};

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

        context.add_precondition(Box::new(task));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Precondition for ValidateChainIdsTask {
    fn id(&self) -> TaskId {
        "validate_chain_ids".into()
    }

    async fn check(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run_once(stop_receiver.0).await
    }
}

use zksync_node_sync::validate_chain_ids_task::ValidateChainIdsTask;
use zksync_types::{L1ChainId, L2ChainId, SLChainId};

use crate::{
    implementations::resources::{
        eth_interface::{EthInterfaceResource, GatewayEthInterfaceResource},
        main_node_client::MainNodeClientResource,
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for chain ID validation precondition for external node.
/// Ensures that chain IDs are consistent locally, on main node, and on the settlement layer.
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
/// - `GatewayEthInterfaceResource`
/// - `MainNodeClientResource`
///
/// ## Adds preconditions
///
/// - `ValidateChainIdsTask`
#[derive(Debug)]
pub struct ValidateChainIdsLayer {
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
    gateway_chain_id: Option<SLChainId>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub l1_client: EthInterfaceResource,
    pub gateway_client: Option<GatewayEthInterfaceResource>,
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: ValidateChainIdsTask,
}

impl ValidateChainIdsLayer {
    pub fn new(
        l1_chain_id: L1ChainId,
        l2_chain_id: L2ChainId,
        gateway_chain_id: Option<SLChainId>,
    ) -> Self {
        Self {
            l1_chain_id,
            l2_chain_id,
            gateway_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ValidateChainIdsLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "validate_chain_ids_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let EthInterfaceResource(l1_query_client) = input.l1_client;
        let MainNodeClientResource(main_node_client) = input.main_node_client;

        let task = ValidateChainIdsTask::new(
            self.l1_chain_id,
            self.l2_chain_id,
            self.gateway_chain_id,
            l1_query_client,
            main_node_client,
            input.gateway_client.map(|c| c.0),
        );

        Ok(Output { task })
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

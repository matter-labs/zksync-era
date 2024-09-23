use zksync_eth_client::clients::ClientMap;
use zksync_node_sync::validate_chain_ids_task::ValidateChainIdsTask;
use zksync_types::{L2ChainId, SLChainId};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, main_node_client::MainNodeClientResource,
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
/// - `MainNodeClientResource
///
/// ## Adds preconditions
///
/// - `ValidateChainIdsTask`
#[derive(Debug)]
pub struct ValidateChainIdsLayer {
    sl_chain_id: SLChainId,
    l2_chain_id: L2ChainId,
    sl_client_map: ClientMap,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: ValidateChainIdsTask,
}

impl ValidateChainIdsLayer {
    pub fn new(sl_chain_id: SLChainId, l2_chain_id: L2ChainId, sl_client_map: ClientMap) -> Self {
        Self {
            sl_chain_id,
            l2_chain_id,
            sl_client_map,
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
        let EthInterfaceResource(query_client) = input.eth_client;
        let MainNodeClientResource(main_node_client) = input.main_node_client;

        let task = ValidateChainIdsTask::new(
            self.sl_chain_id,
            self.l2_chain_id,
            query_client,
            main_node_client,
            self.sl_client_map,
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

use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::{L1ChainId, L2ChainId};
use zksync_web3_decl::client::{DynClient, L1, L2};

use crate::validate_chain_ids_task::ValidateChainIdsTask;

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
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub l1_client: Box<DynClient<L1>>,
    pub main_node_client: Box<DynClient<L2>>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: ValidateChainIdsTask,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "validate_chain_ids_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let task = ValidateChainIdsTask::new(
            self.l1_chain_id,
            self.l2_chain_id,
            input.l1_client,
            input.main_node_client,
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

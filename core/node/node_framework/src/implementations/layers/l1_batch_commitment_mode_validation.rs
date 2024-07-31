use zksync_commitment_generator::validation_task::L1BatchCommitmentModeValidationTask;
use zksync_types::{commitment::L1BatchCommitmentMode, Address};

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for a prerequisite that checks if the L1 batch commitment mode is valid
/// against L1.
#[derive(Debug)]
pub struct L1BatchCommitmentModeValidationLayer {
    diamond_proxy_addr: Address,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: L1BatchCommitmentModeValidationTask,
}

impl L1BatchCommitmentModeValidationLayer {
    pub fn new(
        diamond_proxy_addr: Address,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            diamond_proxy_addr,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for L1BatchCommitmentModeValidationLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "l1_batch_commitment_mode_validation_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let EthInterfaceResource(query_client) = input.eth_client;
        let task = L1BatchCommitmentModeValidationTask::new(
            self.diamond_proxy_addr,
            self.l1_batch_commit_data_generator_mode,
            query_client,
        );

        Ok(Output { task })
    }
}

#[async_trait::async_trait]
impl Task for L1BatchCommitmentModeValidationTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Precondition
    }

    fn id(&self) -> TaskId {
        "l1_batch_commitment_mode_validation".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).exit_on_success().run(stop_receiver.0).await
    }
}

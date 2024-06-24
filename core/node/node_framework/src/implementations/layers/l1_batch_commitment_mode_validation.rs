use zksync_commitment_generator::validation_task::L1BatchCommitmentModeValidationTask;
use zksync_types::{commitment::L1BatchCommitmentMode, Address};

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    precondition::Precondition,
    service::{ServiceContext, StopReceiver},
    task::TaskId,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for a prerequisite that checks if the L1 batch commitment mode is valid
/// against L1.
///
/// ## Requests resources
///
/// - `EthInterfaceResource`
///
/// ## Adds preconditions
///
/// - `L1BatchCommitmentModeValidationTask`
#[derive(Debug)]
pub struct L1BatchCommitmentModeValidationLayer {
    diamond_proxy_addr: Address,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
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
    fn layer_name(&self) -> &'static str {
        "l1_batch_commitment_mode_validation_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let EthInterfaceResource(query_client) = context.get_resource().await?;
        let task = L1BatchCommitmentModeValidationTask::new(
            self.diamond_proxy_addr,
            self.l1_batch_commit_data_generator_mode,
            query_client,
        );

        context.add_precondition(Box::new(task));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Precondition for L1BatchCommitmentModeValidationTask {
    fn id(&self) -> TaskId {
        "l1_batch_commitment_mode_validation".into()
    }

    async fn check(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).exit_on_success().run(stop_receiver.0).await
    }
}

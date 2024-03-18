use zksync_config::ContractVerifierConfig;
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ContractVerificationApiLayer(pub ContractVerifierConfig);

#[derive(Debug)]
pub struct ContractVerificationApiTask {
    config: ContractVerifierConfig,
}

#[async_trait::async_trait]
impl WiringLayer for ContractVerificationApiLayer {
    fn layer_name(&self) -> &'static str {
        "contract_verification_api_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for ContractVerificationApiTask {
    fn name(&self) -> &'static str {
        "contract_verification_api"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        todo!()
    }
}

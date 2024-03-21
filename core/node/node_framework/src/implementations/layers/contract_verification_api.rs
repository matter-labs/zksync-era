use zksync_config::configs::api::ContractVerificationApiConfig;
use zksync_dal::{ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{MasterPoolResource, ReplicaPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ContractVerificationApiLayer(pub ContractVerificationApiConfig);

#[async_trait::async_trait]
impl WiringLayer for ContractVerificationApiLayer {
    fn layer_name(&self) -> &'static str {
        "contract_verification_api_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context
            .get_resource::<MasterPoolResource>()
            .await?
            .get()
            .await?;
        let replica_pool = context
            .get_resource::<ReplicaPoolResource>()
            .await?
            .get()
            .await?;
        context.add_task(Box::new(ContractVerificationApiTask {
            master_pool,
            replica_pool,
            config: self.0,
        }));
        Ok(())
    }
}

#[derive(Debug)]
pub struct ContractVerificationApiTask {
    master_pool: ConnectionPool<Core>,
    replica_pool: ConnectionPool<Core>,
    config: ContractVerificationApiConfig,
}

#[async_trait::async_trait]
impl Task for ContractVerificationApiTask {
    fn name(&self) -> &'static str {
        "contract_verification_api"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_core::api_server::contract_verification::start_server(
            self.master_pool,
            self.replica_pool,
            self.config,
            stop_receiver.0,
        )
        .await
    }
}

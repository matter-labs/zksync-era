use zksync_config::ContractVerifierConfig;
use zksync_dal::{ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource, ReplicaPool},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for contract verification
///
/// Responsible for initialization of the contract verification server.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `PoolResource<ReplicaPool>`
///
/// ## Adds tasks
///
/// - `ContractVerificationApiTask`
#[derive(Debug)]
pub struct ContractVerificationApiLayer(pub ContractVerifierConfig);

#[async_trait::async_trait]
impl WiringLayer for ContractVerificationApiLayer {
    fn layer_name(&self) -> &'static str {
        "contract_verification_api_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;
        let replica_pool = context
            .get_resource::<PoolResource<ReplicaPool>>()?
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
    config: ContractVerifierConfig,
}

#[async_trait::async_trait]
impl Task for ContractVerificationApiTask {
    fn id(&self) -> TaskId {
        "contract_verification_api".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_contract_verification_server::start_server(
            self.master_pool,
            self.replica_pool,
            self.config,
            stop_receiver.0,
        )
        .await
    }
}

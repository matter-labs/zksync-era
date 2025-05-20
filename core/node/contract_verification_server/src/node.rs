use zksync_config::ContractVerifierConfig;
use zksync_dal::{
    node::{MasterPool, PoolResource, ReplicaPool},
    ConnectionPool, Core,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for contract verification
///
/// Responsible for initialization of the contract verification server.
#[derive(Debug)]
pub struct ContractVerificationApiLayer(pub ContractVerifierConfig);

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub contract_verification_api_task: ContractVerificationApiTask,
}

#[async_trait::async_trait]
impl WiringLayer for ContractVerificationApiLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "contract_verification_api_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;
        let replica_pool = input.replica_pool.get().await?;
        let contract_verification_api_task = ContractVerificationApiTask {
            master_pool,
            replica_pool,
            config: self.0,
        };
        Ok(Output {
            contract_verification_api_task,
        })
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
        crate::start_server(
            self.master_pool,
            self.replica_pool,
            self.config.bind_addr(),
            stop_receiver.0,
        )
        .await
    }
}

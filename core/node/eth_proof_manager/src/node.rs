use std::sync::Arc;

use zksync_config::configs::{
    contracts::chain::ProofManagerContracts, eth_proof_manager::EthProofManagerConfig,
};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::{
    clients::{DynClient, L1},
    web3_decl::client::Network,
};
use zksync_eth_watch::GetLogsClient;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;

use crate::{client::ProofManagerClient, EthProofManager};

/// Wiring layer for ethereum watcher
///
/// Responsible for initializing and running of [`EthWatch`] component, that polls the Ethereum node for the relevant events,
/// such as priority operations (aka L1 transactions), protocol upgrades etc.
#[derive(Debug)]
pub struct EthProofManagerLayer {
    eth_proof_manager_config: EthProofManagerConfig,
    eth_proof_manager_contracts: ProofManagerContracts,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    eth_client: Box<DynClient<L1>>,
    blob_store: Arc<dyn ObjectStore>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    eth_proof_manager: EthProofManager,
}

impl EthProofManagerLayer {
    pub fn new(
        eth_proof_manager_config: EthProofManagerConfig,
        eth_proof_manager_contracts: ProofManagerContracts,
    ) -> Self {
        Self {
            eth_proof_manager_config,
            eth_proof_manager_contracts,
        }
    }

    fn create_client<Net: Network>(
        &self,
        client: Box<DynClient<Net>>,
        contracts: &ProofManagerContracts,
    ) -> ProofManagerClient<Net>
    where
        Box<DynClient<Net>>: GetLogsClient,
    {
        ProofManagerClient::new(
            client,
            contracts.proxy_addr,
            self.eth_proof_manager_config.clone(),
        )
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthProofManagerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_proof_manager_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;

        tracing::info!(
            "Proof manager address: {:#?}, proxy address: {:#?}",
            self.eth_proof_manager_contracts.proof_manager_addr,
            self.eth_proof_manager_contracts.proxy_addr
        );

        let client = self.create_client(input.eth_client, &self.eth_proof_manager_contracts);

        let eth_proof_manager = EthProofManager::new(
            Box::new(client),
            main_pool,
            input.blob_store,
            self.eth_proof_manager_config,
        );

        Ok(Output { eth_proof_manager })
    }
}

#[async_trait::async_trait]
impl Task for EthProofManager {
    fn id(&self) -> TaskId {
        "eth_proof_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

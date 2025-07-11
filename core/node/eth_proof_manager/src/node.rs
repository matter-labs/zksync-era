use std::sync::Arc;

use zksync_config::configs::{
    contracts::chain::ProofManagerContracts,
    eth_proof_manager::EthProofManagerConfig,
    wallets::{Wallet, Wallets},
};
use zksync_contracts::proof_manager_contract;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::clients::{DynClient, SigningClient, L2};
use zksync_eth_signer::PrivateKeySigner;
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;
use zksync_types::{L1ChainId, L2ChainId};

use crate::{client::ProofManagerClient, EthProofManager};

/// Wiring layer for proof manager.
#[derive(Debug)]
pub struct EthProofManagerLayer {
    eth_proof_manager_config: EthProofManagerConfig,
    eth_proof_manager_contracts: ProofManagerContracts,
    wallets_config: Wallets,
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    eth_client: Box<DynClient<L2>>,
    blob_store: Arc<dyn ObjectStore>,
    gas_adjuster: Arc<dyn TxParamsProvider>,
}

#[derive(IntoContext)]
pub struct Output {
    #[context(task)]
    eth_proof_manager: EthProofManager,
}

impl EthProofManagerLayer {
    pub fn new(
        eth_proof_manager_config: EthProofManagerConfig,
        eth_proof_manager_contracts: ProofManagerContracts,
        wallets_config: Wallets,
        l1_chain_id: L1ChainId,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            eth_proof_manager_config,
            eth_proof_manager_contracts,
            wallets_config,
            l1_chain_id,
            l2_chain_id,
        }
    }

    fn create_client(
        &self,
        client: Box<DynClient<L2>>,
        gas_adjuster: Arc<dyn TxParamsProvider>,
        contracts: &ProofManagerContracts,
        owner_wallet: Wallet,
    ) -> ProofManagerClient {
        let operator_private_key = owner_wallet.private_key().clone();
        let operator_address = operator_private_key.address();
        let signer = PrivateKeySigner::new(operator_private_key);
        tracing::info!("Operator address: {operator_address:?}");

        let eth_client = SigningClient::new(
            client,
            proof_manager_contract(),
            operator_address,
            signer,
            contracts.proxy_addr,
            self.eth_proof_manager_config
                .default_priority_fee_per_gas
                .into(),
            self.l1_chain_id.into(),
        );

        ProofManagerClient::new(
            Box::new(eth_client),
            gas_adjuster,
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

        let client = self.create_client(
            input.eth_client,
            input.gas_adjuster,
            &self.eth_proof_manager_contracts,
            self.wallets_config
                .eth_proof_manager
                .clone()
                .expect("Eth proof manager wallet is required"),
        );

        let eth_proof_manager = EthProofManager::new(
            Box::new(client),
            main_pool,
            input.blob_store,
            self.eth_proof_manager_config.clone(),
            self.l2_chain_id,
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

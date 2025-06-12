use std::sync::Arc;

use zksync_config::configs::{eth_proof_manager::EthProofManagerConfig, wallets::Wallets};
use zksync_contracts::{chain_admin_contract, eth_proof_manager_contract, getters_facet_contract};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::{
    clients::{PKSigningClient, SigningClient},
    node::contracts::{L1ChainContractsResource, L1EcosystemContractsResource},
    web3_decl::client::{DynClient, L1},
};
use zksync_external_price_api::{NoOpPriceApiClient, PriceApiClient};
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;
use zksync_shared_resources::contracts::EthProofManagerContractsResource;
use zksync_types::L2ChainId;

use crate::EthProofManager;

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for orchestrating communications with external API feeds to get ETH<->BaseToken
/// conversion ratios and persisting them both in the DB and in the L1.
#[derive(Debug)]
pub struct EthProofManagerLayer {
    config: EthProofManagerConfig,
    proof_data_handler_config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    wallets_config: Wallets,
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    blob_store: Arc<dyn ObjectStore>,
    eth_client: Box<DynClient<L1>>,
    tx_params: Arc<dyn TxParamsProvider>,
    proof_manager_contracts: EthProofManagerContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    eth_proof_manager: EthProofManager,
}

impl EthProofManagerLayer {
    pub fn new(
        config: EthProofManagerConfig,
        proof_data_handler_config: ProofDataHandlerConfig,
        blob_store: Arc<dyn ObjectStore>,
        wallets_config: Wallets,
        l1_chain_id: L1ChainId,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            config,
            blob_store,
            wallets_config,
            l1_chain_id,
            l2_chain_id,
            proof_data_handler_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthProofManagerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_proof_manager"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;

        let client = self
            .wallets_config
            .eth_proof_manager_owner
            .map(|eth_proof_manager_owner| {
                let eth_proof_manager_owner_private_key = eth_proof_manager_owner.private_key();
                let eth_proof_manager_owner_address = eth_proof_manager_owner.address();

                SigningClient::<PrivateKeySigner, L1>::new(
                    input.eth_client.for_component("eth_proof_manager"),
                    eth_proof_manager_contract(),
                    eth_proof_manager_owner_address,
                    eth_proof_manager_owner_private_key,
                    input.proof_manager_contracts.proof_manager_addr,
                    self.config.default_priority_fee_per_gas.into(),
                    self.l1_chain_id.into(),
                )
            })
            .expect("Eth proof manager owner wallet is required");

        let eth_proof_manager = EthProofManager::new(
            master_pool,
            input.blob_store,
            client,
            eth_proof_manager_contracts.proof_manager_address,
            eth_proof_manager_contracts.proof_manager_abi,
            input.config.proof_data_handler,
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

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

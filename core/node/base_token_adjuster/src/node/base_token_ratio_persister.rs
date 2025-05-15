use zksync_config::configs::{base_token_adjuster::BaseTokenAdjusterConfig, wallets::Wallets};
use zksync_contracts::{chain_admin_contract, getters_facet_contract};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::{
    clients::PKSigningClient,
    node::contracts::{L1ChainContractsResource, L1EcosystemContractsResource},
    web3_decl::client::{DynClient, L1},
};
use zksync_node_fee_model::node::TxParamsResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::L1ChainId;

use super::resources::PriceAPIClientResource;
use crate::{BaseTokenL1Behaviour, BaseTokenRatioPersister, UpdateOnL1Params};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for orchestrating communications with external API feeds to get ETH<->BaseToken
/// conversion ratios and persisting them both in the DB and in the L1.
#[derive(Debug)]
pub struct BaseTokenRatioPersisterLayer {
    config: BaseTokenAdjusterConfig,
    wallets_config: Wallets,
    l1_chain_id: L1ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub price_api_client: PriceAPIClientResource,
    pub eth_client: Box<DynClient<L1>>,
    pub tx_params: TxParamsResource,
    pub l1_contracts: L1ChainContractsResource,
    pub l1_ecosystem_contracts: L1EcosystemContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub persister: BaseTokenRatioPersister,
}

impl BaseTokenRatioPersisterLayer {
    pub fn new(
        config: BaseTokenAdjusterConfig,
        wallets_config: Wallets,
        l1_chain_id: L1ChainId,
    ) -> Self {
        Self {
            config,
            wallets_config,
            l1_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenRatioPersisterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "base_token_ratio_persister"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool.get().await?;

        let price_api_client = input.price_api_client;
        let base_token_addr = input.l1_ecosystem_contracts.0.base_token_address;

        let l1_behaviour = self
            .wallets_config
            .token_multiplier_setter
            .map(|token_multiplier_setter| {
                let tms_private_key = token_multiplier_setter.wallet.private_key();
                let tms_address = token_multiplier_setter.wallet.address();
                let l1_diamond_proxy_addr = input
                    .l1_contracts
                    .0
                    .chain_contracts_config
                    .diamond_proxy_addr;

                let signing_client = PKSigningClient::new_raw(
                    tms_private_key.clone(),
                    l1_diamond_proxy_addr,
                    self.config.default_priority_fee_per_gas,
                    self.l1_chain_id.into(),
                    input.eth_client.for_component("base_token_adjuster"),
                );
                BaseTokenL1Behaviour::UpdateOnL1 {
                    params: UpdateOnL1Params {
                        eth_client: Box::new(signing_client),
                        gas_adjuster: input.tx_params.0,
                        token_multiplier_setter_account_address: tms_address,
                        chain_admin_contract: chain_admin_contract(),
                        getters_facet_contract: getters_facet_contract(),
                        diamond_proxy_contract_address: l1_diamond_proxy_addr,
                        chain_admin_contract_address: input.l1_ecosystem_contracts.0.chain_admin,
                        config: self.config.clone(),
                    },
                    last_persisted_l1_ratio: None,
                }
            })
            .unwrap_or(BaseTokenL1Behaviour::NoOp);

        let persister = BaseTokenRatioPersister::new(
            master_pool,
            self.config,
            base_token_addr,
            price_api_client.0,
            l1_behaviour,
        );

        Ok(Output { persister })
    }
}

#[async_trait::async_trait]
impl Task for BaseTokenRatioPersister {
    fn id(&self) -> TaskId {
        "base_token_ratio_persister".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

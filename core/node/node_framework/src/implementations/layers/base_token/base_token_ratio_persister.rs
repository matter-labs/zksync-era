use zksync_base_token_adjuster::BaseTokenRatioPersister;
use zksync_config::{
    configs::{base_token_adjuster::BaseTokenAdjusterConfig, wallets::Wallets},
    ContractsConfig,
};
use zksync_eth_client::clients::PKSigningClient;
use zksync_types::L1ChainId;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        l1_tx_params::TxParamsResource,
        pools::{MasterPool, PoolResource},
        price_api_client::PriceAPIClientResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for orchestrating communications with external API feeds to get ETH<->BaseToken
/// conversion ratios and persisting them both in the DB and in the L1.
#[derive(Debug)]
pub struct BaseTokenRatioPersisterLayer {
    config: BaseTokenAdjusterConfig,
    contracts_config: ContractsConfig,
    wallets_config: Wallets,
    l1_chain_id: L1ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub price_api_client: PriceAPIClientResource,
    pub eth_client: EthInterfaceResource,
    pub tx_params: TxParamsResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub persister: BaseTokenRatioPersister,
}

impl BaseTokenRatioPersisterLayer {
    pub fn new(
        config: BaseTokenAdjusterConfig,
        contracts_config: ContractsConfig,
        wallets_config: Wallets,
        l1_chain_id: L1ChainId,
    ) -> Self {
        Self {
            config,
            contracts_config,
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
        let base_token_addr = self
            .contracts_config
            .base_token_addr
            .expect("base token address is not set");
        let diamond_proxy_contract_address = self.contracts_config.diamond_proxy_addr;
        let chain_admin_contract_address = self.contracts_config.chain_admin_addr;
        let token_multiplier_setter_wallet = self
            .wallets_config
            .token_multiplier_setter
            .expect("base token adjuster wallet is not set")
            .wallet;

        let tms_private_key = token_multiplier_setter_wallet.private_key();
        let tms_address = token_multiplier_setter_wallet.address();
        let EthInterfaceResource(query_client) = input.eth_client;

        let signing_client = PKSigningClient::new_raw(
            tms_private_key.clone(),
            self.contracts_config.diamond_proxy_addr,
            self.config.default_priority_fee_per_gas,
            #[allow(clippy::useless_conversion)]
            self.l1_chain_id.into(),
            query_client.clone().for_component("base_token_adjuster"),
        );

        let persister = BaseTokenRatioPersister::new(
            master_pool,
            self.config,
            base_token_addr,
            price_api_client.0,
            Box::new(signing_client),
            input.tx_params.0,
            tms_address,
            diamond_proxy_contract_address,
            chain_admin_contract_address,
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

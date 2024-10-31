use anyhow::Context;
use futures::TryFutureExt;
use zksync_config::{configs::gateway::GatewayChainConfig, ContractsConfig, EthWatchConfig};
use zksync_contracts::chain_admin_contract;
use zksync_eth_client::EthInterface;
use zksync_eth_watch::{EthHttpQueryClient, EthWatch};
use zksync_types::{
    abi::ZkChainSpecificUpgradeData, ethabi::ParamType, ethereum, settlement::SettlementMode,
    web3::CallRequest, Address, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
};
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    implementations::resources::{
        eth_interface::{EthInterfaceResource, GatewayEthInterfaceResource},
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for ethereum watcher
///
/// Responsible for initializing and running of [`EthWatch`] component, that polls the Ethereum node for the relevant events,
/// such as priority operations (aka L1 transactions), protocol upgrades etc.
#[derive(Debug)]
pub struct EthWatchLayer {
    eth_watch_config: EthWatchConfig,
    contracts_config: ContractsConfig,
    gateway_contracts_config: Option<GatewayChainConfig>,
    settlement_mode: SettlementMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub gateway_client: Option<GatewayEthInterfaceResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_watch: EthWatch,
}

impl EthWatchLayer {
    pub fn new(
        eth_watch_config: EthWatchConfig,
        contracts_config: ContractsConfig,
        gateway_contracts_config: Option<GatewayChainConfig>,
        settlement_mode: SettlementMode,
    ) -> Self {
        Self {
            eth_watch_config,
            contracts_config,
            gateway_contracts_config,
            settlement_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthWatchLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_watch_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        let client = input.eth_client.0;

        let (base_token_name, base_token_symbol) =
            if let Some(base_token_addr) = self.contracts_config.base_token_addr {
                let (name, symbol) = get_token_metadata(&client, base_token_addr)
                    .await
                    .map_err(|e| WiringError::Internal(e))?;
                (Some(name), Some(symbol))
            } else {
                (None, None)
            };

        //  = if self.contracts_config.base_token_addr

        let sl_diamond_proxy_addr = if self.settlement_mode.is_gateway() {
            self.gateway_contracts_config
                .clone()
                .unwrap()
                .diamond_proxy_addr
        } else {
            self.contracts_config.diamond_proxy_addr
        };
        tracing::info!(
            "Diamond proxy address ethereum: {}",
            self.contracts_config.diamond_proxy_addr
        );
        tracing::info!(
            "Diamond proxy address settlement_layer: {}",
            sl_diamond_proxy_addr
        );

        let l1_client = EthHttpQueryClient::new(
            client,
            self.contracts_config.diamond_proxy_addr,
            self.contracts_config
                .ecosystem_contracts
                .as_ref()
                .and_then(|a| a.l1_bytecodes_supplier_addr),
            self.contracts_config
                .ecosystem_contracts
                .map(|a| a.state_transition_proxy_addr),
            self.contracts_config.chain_admin_addr,
            self.contracts_config.governance_addr,
            self.eth_watch_config.confirmations_for_eth_event,
        );

        let sl_client = if self.settlement_mode.is_gateway() {
            let gateway_client = input.gateway_client.unwrap().0;
            let contracts_config = self.gateway_contracts_config.unwrap();
            EthHttpQueryClient::new(
                gateway_client,
                contracts_config.diamond_proxy_addr,
                None,
                Some(contracts_config.state_transition_proxy_addr),
                contracts_config.chain_admin_addr,
                contracts_config.governance_addr,
                self.eth_watch_config.confirmations_for_eth_event,
            )
        } else {
            l1_client.clone()
        };

        let eth_watch = EthWatch::new(
            &chain_admin_contract(),
            Box::new(l1_client),
            Box::new(sl_client),
            main_pool,
            self.eth_watch_config.poll_interval(),
            ZkChainSpecificUpgradeData::from_partial_components(
                self.contracts_config.base_token_asset_id,
                self.contracts_config.l2_legacy_shared_bridge_addr,
                self.contracts_config.predeployed_l2_weth_token_address,
                self.contracts_config.base_token_addr,
                base_token_name,
                base_token_symbol,
            ),
        )
        .await?;

        Ok(Output { eth_watch })
    }
}

/// Returns token's name and symbol
async fn get_token_metadata(
    client: &Box<DynClient<L1>>,
    addr: Address,
) -> anyhow::Result<(String, String)> {
    if addr == SHARED_BRIDGE_ETHER_TOKEN_ADDRESS {
        return Ok((String::from("Ether"), String::from("ETH")));
    }

    // It is an ERC20 token, so we'll have to call it on L1.

    let name_selector = zksync_types::ethabi::short_signature("name", &[]);
    let symbol_selector = zksync_types::ethabi::short_signature("symbol", &[]);

    let name_request = CallRequest {
        to: Some(addr),
        data: Some(name_selector.into()),
        ..Default::default()
    };

    let symbol_request = CallRequest {
        to: Some(addr),
        data: Some(symbol_selector.into()),
        ..Default::default()
    };

    let name_result = client.call_contract_function(name_request, None).await?;
    let symbol_result = client.call_contract_function(symbol_request, None).await?;

    let name = zksync_types::ethabi::decode(&[ParamType::String], &name_result.0)?;
    let symbol = zksync_types::ethabi::decode(&[ParamType::String], &symbol_result.0)?;

    Ok((name[0].to_string(), symbol[0].to_string()))
}

#[async_trait::async_trait]
impl Task for EthWatch {
    fn id(&self) -> TaskId {
        "eth_watch".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

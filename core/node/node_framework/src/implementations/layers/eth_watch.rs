use anyhow::Context;
use zksync_config::{configs::gateway::GatewayChainConfig, ContractsConfig, EthWatchConfig};
use zksync_contracts::chain_admin_contract;
use zksync_eth_watch::{EthHttpQueryClient, EthWatch, L2EthClient};
use zksync_system_constants::L2_MESSAGE_ROOT_ADDRESS;
use zksync_types::{settlement::SettlementMode, L2ChainId, SLChainId};

use crate::{
    implementations::resources::{
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
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
    gateway_chain_config: Option<GatewayChainConfig>,
    settlement_mode: SettlementMode,
    chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub gateway_client: Option<L2InterfaceResource>,
    pub dependency_chain_clients: Option<L2InterfaceResource>,
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
        gateway_chain_config: Option<GatewayChainConfig>,
        settlement_mode: SettlementMode,
        chain_id: L2ChainId,
    ) -> Self {
        Self {
            eth_watch_config,
            contracts_config,
            gateway_chain_config,
            settlement_mode,
            chain_id,
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
        // println!("input in wiring: {:?}", input);
        let main_pool = input.master_pool.get().await?;
        let client = input.eth_client.0;

        let sl_diamond_proxy_addr = if self.settlement_mode.is_gateway() {
            self.gateway_chain_config
                .clone()
                .context("Lacking `gateway_contracts_config`")?
                .diamond_proxy_addr
        } else {
            self.contracts_config.diamond_proxy_addr
        };
        tracing::info!(
            "Diamond proxy address ethereum: {:#?}",
            self.contracts_config.diamond_proxy_addr
        );
        tracing::info!(
            "Diamond proxy address settlement_layer: {:#?}",
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
                .as_ref()
                .and_then(|a| a.l1_wrapped_base_token_store),
            self.contracts_config.l1_shared_bridge_proxy_addr,
            self.contracts_config
                .ecosystem_contracts
                .as_ref()
                .and_then(|a| a.message_root_proxy_addr),
            self.contracts_config
                .ecosystem_contracts
                .as_ref()
                .map(|a| a.state_transition_proxy_addr),
            self.contracts_config.chain_admin_addr,
            self.contracts_config.governance_addr,
            self.eth_watch_config.confirmations_for_eth_event,
            self.chain_id,
            false,
        );
        // println!("l1_message_root_address 2: {:?}", self.contracts_config.l1_message_root_address);

        let sl_l2_client: Option<Box<dyn L2EthClient>> = if let Some(gateway_client) =
            input.gateway_client
        {
            let contracts_config: GatewayChainConfig = self.gateway_chain_config.clone().unwrap();
            Some(Box::new(EthHttpQueryClient::new(
                gateway_client.0,
                contracts_config.diamond_proxy_addr,
                // Only present on L1.
                None,
                // Only present on L1.
                None,
                // Only present on L1.
                None,
                Some(L2_MESSAGE_ROOT_ADDRESS),
                Some(contracts_config.state_transition_proxy_addr),
                contracts_config.chain_admin_addr,
                contracts_config.governance_addr,
                self.eth_watch_config.confirmations_for_eth_event,
                self.chain_id,
                false,
            )))
        } else {
            None
        };

        let dependency_l2_chain_clients: Option<Vec<Box<dyn L2EthClient>>> =
            if let Some(dependency_chain_client) = input.dependency_chain_clients.clone() {
                let mut clients: Vec<Box<dyn L2EthClient>> = Vec::new();
                // let contracts_config: GatewayChainConfig = self.gateway_chain_config.unwrap();
                let dependency_chain_clients = vec![dependency_chain_client];
                for dependency_chain_client in dependency_chain_clients {
                    let client = Box::new(EthHttpQueryClient::new(
                        dependency_chain_client.0,
                        L2_MESSAGE_ROOT_ADDRESS,
                        None,
                        None,
                        None,
                        Some(L2_MESSAGE_ROOT_ADDRESS),
                        Some(L2_MESSAGE_ROOT_ADDRESS),
                        Some(L2_MESSAGE_ROOT_ADDRESS),
                        L2_MESSAGE_ROOT_ADDRESS,
                        self.eth_watch_config.confirmations_for_eth_event,
                        self.chain_id,
                        true,
                    ));
                    clients.push(client);
                }
                Some(clients)
            } else {
                None
            };
        println!(
            "dependency_l2_chain_clients: {:?}",
            input.dependency_chain_clients
        );
        println!("sl_l2_client: {:?}", sl_l2_client);
        let eth_watch = EthWatch::new(
            &chain_admin_contract(),
            Box::new(l1_client),
            sl_l2_client,
            dependency_l2_chain_clients,
            main_pool,
            self.eth_watch_config.poll_interval(),
            self.chain_id,
        )
        .await?;

        Ok(Output { eth_watch })
    }
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

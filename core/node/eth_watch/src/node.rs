use zksync_config::{
    configs::contracts::{ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts},
    EthWatchConfig,
};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::node::contracts::{
    L1ChainContractsResource, L1EcosystemContractsResource, SettlementLayerContractsResource,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::L2ChainId;
use zksync_web3_decl::{
    client::{DynClient, Network},
    node::{EthInterfaceResource, SettlementLayerClient, SettlementModeResource},
};

use crate::{EthHttpQueryClient, EthWatch, GetLogsClient, ZkSyncExtentionEthClient};

/// Wiring layer for ethereum watcher
///
/// Responsible for initializing and running of [`EthWatch`] component, that polls the Ethereum node for the relevant events,
/// such as priority operations (aka L1 transactions), protocol upgrades etc.
#[derive(Debug)]
pub struct EthWatchLayer {
    eth_watch_config: EthWatchConfig,
    chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub l1_contracts: L1ChainContractsResource,
    pub sl_contracts: SettlementLayerContractsResource,
    pub l1_ecosystem_contracts: L1EcosystemContractsResource,
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub client: SettlementLayerClient,
    pub settlement_mode: SettlementModeResource,
    pub dependency_chain_clients: Option<SettlementLayerClient>, //
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub eth_watch: EthWatch,
}

impl EthWatchLayer {
    pub fn new(eth_watch_config: EthWatchConfig, chain_id: L2ChainId) -> Self {
        Self {
            eth_watch_config,
            chain_id,
        }
    }

    fn create_client<Net: Network>(
        &self,
        client: Box<DynClient<Net>>,
        contracts: &SettlementLayerSpecificContracts,
        l1_ecosystem_contracts: &L1SpecificContracts,
    ) -> EthHttpQueryClient<Net>
    where
        Box<DynClient<Net>>: GetLogsClient,
    {
        EthHttpQueryClient::new(
            client,
            contracts.chain_contracts_config.diamond_proxy_addr,
            l1_ecosystem_contracts.bytecodes_supplier_addr,
            l1_ecosystem_contracts.wrapped_base_token_store,
            l1_ecosystem_contracts.shared_bridge,
            contracts.ecosystem_contracts.state_transition_proxy_addr,
            l1_ecosystem_contracts.chain_admin,
            l1_ecosystem_contracts.server_notifier_addr,
            self.eth_watch_config.confirmations_for_eth_event,
            self.chain_id,
            false,
        )
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

        tracing::info!(
            "Diamond proxy address settlement_layer: {:#?}",
            input
                .sl_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr
        );

        let l1_client = self.create_client(
            client,
            &input.l1_contracts.0,
            &input.l1_ecosystem_contracts.0,
        );
        // println!("l1_message_root_address 2: {:?}", self.contracts_config.l1_message_root_address);

        let sl_l2_client: Box<dyn ZkSyncExtentionEthClient> = match input.client {
            SettlementLayerClient::L1(client) => Box::new(self.create_client(
                client,
                &input.sl_contracts.0,
                &input.l1_ecosystem_contracts.0,
            )),
            SettlementLayerClient::L2(client) => Box::new(self.create_client(
                client,
                &input.sl_contracts.0,
                &input.l1_ecosystem_contracts.0,
            )),
        };

        let dependency_l2_chain_clients: Option<Vec<Box<dyn ZkSyncExtentionEthClient>>> =
            // if let Some(dependency_chain_client) = input.dependency_chain_clients.clone() {
            //     let mut clients: Vec<Box<dyn L2EthClient>> = Vec::new();
            //     // let contracts_config: GatewayChainConfig = self.gateway_chain_config.unwrap();
            //     let dependency_chain_clients = vec![dependency_chain_client];
            //     for dependency_chain_client in dependency_chain_clients {
            //         let client = Box::new(EthHttpQueryClient::new(
            //             dependency_chain_client.0,
            //             L2_MESSAGE_ROOT_ADDRESS,
            //             None,
            //             None,
            //             None,
            //             Some(L2_MESSAGE_ROOT_ADDRESS),
            //             Some(L2_MESSAGE_ROOT_ADDRESS),
            //             Some(L2_MESSAGE_ROOT_ADDRESS),
            //             L2_MESSAGE_ROOT_ADDRESS,
            //             self.eth_watch_config.confirmations_for_eth_event,
            //             self.chain_id,
            //             true,
            //         ));
            //         clients.push(client);
            //     }
            //     Some(clients)
            // } else {
                None;
        // };
        println!(
            "dependency_l2_chain_clients: {:?}",
            input.dependency_chain_clients
        );
        println!("sl_l2_client : {:?}", sl_l2_client);
        let eth_watch = EthWatch::new(
            Box::new(l1_client),
            sl_l2_client,
            input.settlement_mode.settlement_layer_for_sending_txs(),
            dependency_l2_chain_clients, //
            main_pool,
            self.eth_watch_config.eth_node_poll_interval,
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

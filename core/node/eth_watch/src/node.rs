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
    client::{DynClient, Network, L1},
    node::{SettlementLayerClient, SettlementModeResource},
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
    l1_contracts: L1ChainContractsResource,
    sl_contracts: SettlementLayerContractsResource,
    l1_ecosystem_contracts: L1EcosystemContractsResource,
    master_pool: PoolResource<MasterPool>,
    eth_client: Box<DynClient<L1>>,
    client: SettlementLayerClient,
    settlement_mode: SettlementModeResource,
    dependency_chain_clients: Option<SettlementLayerClient>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    eth_watch: EthWatch,
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
            l1_ecosystem_contracts.message_root,
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
        let main_pool = input.master_pool.get().await?;

        tracing::info!(
            "Diamond proxy address settlement_layer: {:#?}",
            input
                .sl_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr
        );

        let l1_client = self.create_client(
            input.eth_client,
            &input.l1_contracts.0,
            &input.l1_ecosystem_contracts.0,
        );

        let sl_l2_client: Box<dyn ZkSyncExtentionEthClient> = match input.client {
            SettlementLayerClient::L1(client) => Box::new(self.create_client(
                client,
                &input.sl_contracts.0,
                &input.l1_ecosystem_contracts.0,
            )),
            SettlementLayerClient::Gateway(client) => Box::new(self.create_client(
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
            self.eth_watch_config.event_expiration_blocks,
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

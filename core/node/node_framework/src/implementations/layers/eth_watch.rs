use zksync_config::{
    configs::contracts::{ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts},
    EthWatchConfig,
};
use zksync_eth_watch::{EthHttpQueryClient, EthWatch, GetLogsClient, ZkSyncExtentionEthClient};
use zksync_types::L2ChainId;
use zksync_web3_decl::client::{DynClient, Network};

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource,
            SettlementLayerContractsResource,
        },
        eth_interface::{
            EthInterfaceResource, SettlementLayerClient, SettlementLayerClientResource,
        },
        pools::{MasterPool, PoolResource},
        settlement_layer::SettlementModeResource,
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
    chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub l1_contracts: L1ChainContractsResource,
    pub contracts_resource: SettlementLayerContractsResource,
    pub l1ecosystem_contracts_resource: L1EcosystemContractsResource,
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub client: SettlementLayerClientResource,
    pub settlement_mode: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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
        contracts_resource: &SettlementLayerSpecificContracts,
        l1_ecosystem_contracts_resource: &L1SpecificContracts,
    ) -> EthHttpQueryClient<Net>
    where
        Box<DynClient<Net>>: GetLogsClient,
    {
        EthHttpQueryClient::new(
            client,
            contracts_resource.chain_contracts_config.diamond_proxy_addr,
            l1_ecosystem_contracts_resource.bytecodes_supplier_addr,
            l1_ecosystem_contracts_resource.wrapped_base_token_store,
            l1_ecosystem_contracts_resource.shared_bridge,
            contracts_resource
                .ecosystem_contracts
                .message_root_proxy_addr,
            contracts_resource
                .ecosystem_contracts
                .state_transition_proxy_addr,
            l1_ecosystem_contracts_resource.chain_admin,
            l1_ecosystem_contracts_resource.server_notifier_addr,
            self.eth_watch_config.confirmations_for_eth_event,
            self.chain_id,
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
                .contracts_resource
                .0
                .chain_contracts_config
                .diamond_proxy_addr
        );

        let l1_client = self.create_client(
            client,
            &input.l1_contracts.0,
            &input.l1ecosystem_contracts_resource.0,
        );
        // println!("l1_message_root_address 2: {:?}", self.contracts_config.l1_message_root_address);

        let sl_l2_client: Box<dyn ZkSyncExtentionEthClient> = match input.client.0 {
            SettlementLayerClient::L1(client) => Box::new(self.create_client(
                client,
                &input.contracts_resource.0,
                &input.l1ecosystem_contracts_resource.0,
            )),
            SettlementLayerClient::L2(client) => Box::new(self.create_client(
                client,
                &input.contracts_resource.0,
                &input.l1ecosystem_contracts_resource.0,
            )),
        };

        let eth_watch = EthWatch::new(
            Box::new(l1_client),
            sl_l2_client,
            input.settlement_mode.settlement_layer_for_sending_txs(),
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

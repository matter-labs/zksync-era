use anyhow::Context;
use zksync_config::EthWatchConfig;
use zksync_eth_watch::{EthHttpQueryClient, EthWatch, ZkSyncExtentionEthClient};
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        contracts::ContractsResource,
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
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
    pub contracts_resource: ContractsResource,
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
    pub gateway_client: Option<L2InterfaceResource>,
    pub settlement_mode: SettlementModeResource,
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
        // contracts_config: ContractsConfig,
        // gateway_chain_config: Option<GatewayChainConfig>,
        chain_id: L2ChainId,
    ) -> Self {
        Self {
            eth_watch_config,
            // contracts_config,
            // gateway_chain_config,
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
        let main_pool = input.master_pool.get().await?;
        let client = input.eth_client.0;

        tracing::info!(
            "Diamond proxy address ethereum: {:#?}",
            input
                .contracts_resource
                .0
                .l1_contracts()
                .chain_contracts_config
                .diamond_proxy_addr
        );
        tracing::info!(
            "Diamond proxy address settlement_layer: {:#?}",
            input
                .contracts_resource
                .0
                .current_contracts()
                .chain_contracts_config
                .diamond_proxy_addr
        );

        let l1_client = EthHttpQueryClient::new(
            client,
            input
                .contracts_resource
                .0
                .current_contracts()
                .chain_contracts_config
                .diamond_proxy_addr,
            input
                .contracts_resource
                .0
                .l1_specific_contracts()
                .bytecodes_supplier_addr,
            input
                .contracts_resource
                .0
                .l1_specific_contracts()
                .wrapped_base_token_store,
            input
                .contracts_resource
                .0
                .l1_specific_contracts()
                .shared_bridge,
            Some(
                input
                    .contracts_resource
                    .0
                    .current_contracts()
                    .ecosystem_contracts
                    .state_transition_proxy_addr,
            ),
            input
                .contracts_resource
                .0
                .current_contracts()
                .chain_contracts_config
                .chain_admin,
            input
                .contracts_resource
                .0
                .current_contracts()
                .ecosystem_contracts
                .server_notifier_addr,
            self.eth_watch_config.confirmations_for_eth_event,
            self.chain_id,
        );

        let sl_l2_client: Box<dyn ZkSyncExtentionEthClient> =
            if let Some(gateway_client) = input.gateway_client {
                let contracts_config = input.contracts_resource.0.gateway().unwrap();
                Box::new(EthHttpQueryClient::new(
                    gateway_client.0,
                    contracts_config.chain_contracts_config.diamond_proxy_addr,
                    // Only present on L1.
                    None,
                    // Only present on L1.
                    None,
                    // Only present on L1.
                    None,
                    Some(
                        contracts_config
                            .ecosystem_contracts
                            .state_transition_proxy_addr,
                    ),
                    contracts_config.chain_contracts_config.chain_admin,
                    input
                        .contracts_resource
                        .0
                        .current_contracts()
                        .ecosystem_contracts
                        .server_notifier_addr,
                    self.eth_watch_config.confirmations_for_eth_event,
                    self.chain_id,
                ))
            } else {
                Box::new(l1_client.clone())
            };

        let eth_watch = EthWatch::new(
            Box::new(l1_client),
            sl_l2_client,
            input.settlement_mode.0,
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

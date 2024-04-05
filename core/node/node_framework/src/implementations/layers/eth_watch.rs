use std::time::Duration;

use zksync_config::{ContractsConfig, ETHWatchConfig, GenesisConfig};
use zksync_contracts::governance_contract;
use zksync_core::eth_watch::{client::EthHttpQueryClient, EthWatch};
use zksync_dal::{ConnectionPool, Core};
use zksync_types::{ethabi::Contract, Address};

use crate::{
    implementations::resources::{eth_interface::EthInterfaceResource, pools::MasterPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct EthWatchLayer {
    eth_watch_config: ETHWatchConfig,
    genesis_config: GenesisConfig,
    contracts_config: ContractsConfig,
}

impl EthWatchLayer {
    pub fn new(
        eth_watch_config: ETHWatchConfig,
        contracts_config: ContractsConfig,
        genesis_config: GenesisConfig,
    ) -> Self {
        Self {
            eth_watch_config,
            genesis_config,
            contracts_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthWatchLayer {
    fn layer_name(&self) -> &'static str {
        "eth_watch_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let main_pool = pool_resource.get().await.unwrap();

        let client = context.get_resource::<EthInterfaceResource>().await?.0;

        let state_transition_manager_address = self
            .genesis_config
            .shared_bridge
            .as_ref()
            .map(|a| a.state_transition_proxy_addr);

        let eth_client = EthHttpQueryClient::new(
            client,
            self.contracts_config.diamond_proxy_addr,
            self.genesis_config
                .shared_bridge
                .map(|a| a.transparent_proxy_admin_addr),
            Some(self.contracts_config.governance_addr),
            self.eth_watch_config.confirmations_for_eth_event,
        );
        context.add_task(Box::new(EthWatchTask {
            main_pool,
            client: eth_client,
            governance_contract: Some(governance_contract()),
            state_transition_manager_address,
            diamond_proxy_address: self.contracts_config.diamond_proxy_addr,
            poll_interval: self.eth_watch_config.poll_interval(),
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct EthWatchTask {
    main_pool: ConnectionPool<Core>,
    client: EthHttpQueryClient,
    governance_contract: Option<Contract>,
    state_transition_manager_address: Option<Address>,
    diamond_proxy_address: Address,
    poll_interval: Duration,
}

#[async_trait::async_trait]
impl Task for EthWatchTask {
    fn name(&self) -> &'static str {
        "eth_watch"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let eth_watch = EthWatch::new(
            self.diamond_proxy_address,
            self.state_transition_manager_address,
            self.governance_contract,
            Box::new(self.client),
            self.main_pool,
            self.poll_interval,
        )
        .await;

        eth_watch.run(stop_receiver.0).await
    }
}

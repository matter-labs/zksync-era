use std::time::Duration;

use zksync_config::{ContractsConfig, EthWatchConfig};
use zksync_contracts::governance_contract;
use zksync_dal::{ConnectionPool, Core};
use zksync_eth_watch::{EthHttpQueryClient, EthWatch};
use zksync_types::{ethabi::Contract, Address};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for ethereum watcher
///
/// Responsible for initializing and running of [`EthWatch`] component, that polls the Ethereum node for the relevant events,
/// such as priority operations (aka L1 transactions), protocol upgrades etc.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `EthInterfaceResource`
///
/// ## Adds tasks
///
/// - `EthWatchTask`
#[derive(Debug)]
pub struct EthWatchLayer {
    eth_watch_config: EthWatchConfig,
    contracts_config: ContractsConfig,
}

impl EthWatchLayer {
    pub fn new(eth_watch_config: EthWatchConfig, contracts_config: ContractsConfig) -> Self {
        Self {
            eth_watch_config,
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
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let main_pool = pool_resource.get().await.unwrap();

        let client = context.get_resource::<EthInterfaceResource>()?.0;

        let eth_client = EthHttpQueryClient::new(
            client,
            self.contracts_config.diamond_proxy_addr,
            self.contracts_config
                .ecosystem_contracts
                .map(|a| a.transparent_proxy_admin_addr),
            self.contracts_config.governance_addr,
            self.eth_watch_config.confirmations_for_eth_event,
        );
        context.add_task(EthWatchTask {
            main_pool,
            client: eth_client,
            governance_contract: governance_contract(),
            diamond_proxy_address: self.contracts_config.diamond_proxy_addr,
            poll_interval: self.eth_watch_config.poll_interval(),
        });

        Ok(())
    }
}

#[derive(Debug)]
struct EthWatchTask {
    main_pool: ConnectionPool<Core>,
    client: EthHttpQueryClient,
    governance_contract: Contract,
    diamond_proxy_address: Address,
    poll_interval: Duration,
}

#[async_trait::async_trait]
impl Task for EthWatchTask {
    fn id(&self) -> TaskId {
        "eth_watch".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let eth_watch = EthWatch::new(
            self.diamond_proxy_address,
            &self.governance_contract,
            Box::new(self.client),
            self.main_pool,
            self.poll_interval,
        )
        .await?;

        eth_watch.run(stop_receiver.0).await
    }
}

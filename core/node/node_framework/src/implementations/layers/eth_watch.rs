use zksync_config::{ContractsConfig, EthWatchConfig};
use zksync_contracts::{chain_admin_contract, governance_contract};
use zksync_eth_watch::{EthHttpQueryClient, EthWatch};

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
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
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub eth_client: EthInterfaceResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_watch: EthWatch,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_watch_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        let client = input.eth_client.0;

        let eth_client = EthHttpQueryClient::new(
            client,
            self.contracts_config.diamond_proxy_addr,
            self.contracts_config
                .ecosystem_contracts
                .map(|a| a.state_transition_proxy_addr),
            self.contracts_config.chain_admin_addr,
            self.contracts_config.governance_addr,
            self.eth_watch_config.confirmations_for_eth_event,
        );

        let eth_watch = EthWatch::new(
            self.contracts_config.diamond_proxy_addr,
            &governance_contract(),
            &chain_admin_contract(),
            Box::new(eth_client),
            main_pool,
            self.eth_watch_config.poll_interval(),
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

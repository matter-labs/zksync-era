use std::sync::Arc;

use zksync_config::configs::{
    chain::NetworkConfig,
    eth_sender::{ETHSenderConfig, SenderConfig},
    ContractsConfig, ETHClientConfig, PostgresConfig,
};
use zksync_core::{
    eth_sender::{Aggregator, EthTxAggregator, EthTxManager},
    l1_gas_price::L1TxParamsProvider,
};
use zksync_dal::ConnectionPool;
use zksync_eth_client::{clients::PKSigningClient, BoundEthInterface};

use crate::{
    implementations::resources::{eth_interface::EthInterfaceResource, pools::MasterPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

//eth sender layer
#[derive(Debug)]
pub struct EthSenderLayer {
    postgres_config: PostgresConfig,
    eth_sender_config: ETHSenderConfig,
    contracts_config: ContractsConfig,
    eth_client_config: ETHClientConfig,
    network_config: NetworkConfig,
}

impl EthSenderLayer {
    pub fn new() -> Self {
        Self {
            postgres_config: todo!(),
            eth_sender_config: todo!(),
            contracts_config: todo!(),
            eth_client_config: todo!(),
            network_config: todo!(),
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthSenderLayer {
    fn layer_name(&self) -> &'static str {
        "eth_sender_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let pool = pool_resource.get().await.unwrap();

        let eth_client = context.get_resource::<EthInterfaceResource>().await?;

        let object_store = context.get_resource::<ObjectStoreResource>().await?;

        let eth_client = PKSigningClient::from_config(
            &self.eth_sender_config,
            &self.contracts_config,
            &self.eth_client_config,
        );
        let eth_client_blobs_addr = PKSigningClient::from_config_blobs(
            &self.eth_sender_config,
            &self.contracts_config,
            &self.eth_client_config,
        )
        .map(|k| k.sender_account());

        let aggregator = Aggregator::new(
            self.eth_sender_config.sender.clone(),
            object_store.0,
            eth_client_blobs_addr.is_some(),
            self.eth_sender_config.sender.pubdata_sending_mode.into(),
        );

        let eth_client = context.get_resource::<EthInterfaceResource>().await?.0;

        context
            .add_task(Box::new(EthTxAggregatorTask {
                pool: pool.clone(),
                config: self.eth_sender_config,
                aggregator,
                eth_client,
                timelock_contract_address: self.contracts_config.validator_timelock_addr,
                l1_multicall3_address: self.contracts_config.l1_multicall3_addr,
                main_zksync_contract_address: contracts_config.diamond_proxy_addr,
                rollup_chain_id: self
                    .network_config
                    .as_ref()
                    .context("network_config")?
                    .zksync_network_id,
                custom_commit_sender_addr: eth_client_blobs_addr,
            }))
            .add_task(Box::new(EthTxManagerTask {
                pool,
                config: self.eth_sender_config,
                gas_adjuster: todo!(),
                ethereum_gateway: todo!(),
                ethereum_gateway_blobs: todo!(),
            }));

        Ok(())
    }
}

// eth tx aggregator task
#[derive(Debug)]
struct EthTxAggregatorTask {
    pool: ConnectionPool,
    config: SenderConfig,
    aggregator: Aggregator,
    eth_client: Arc<dyn BoundEthInterface>,
    timelock_contract_address: Address,
    l1_multicall3_address: Address,
    main_zksync_contract_address: Address,
    rollup_chain_id: L2ChainId,
    custom_commit_sender_addr: Option<Address>,
}

#[async_trait::async_trait]
impl Task for EthTxAggregatorTask {
    fn name(&self) -> &'static str {
        "eth_tx_aggregator"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let eth_tx_aggregator_actor = EthTxAggregator::new(
            self.pool,
            self.config,
            self.aggregator,
            self.eth_client,
            self.timelock_contract_address,
            self.l1_multicall3_address,
            self.main_zksync_contract_address,
            self.rollup_chain_id,
            self.custom_commit_sender_addr,
        )
        .await;

        eth_tx_aggregator_actor.run(stop_receiver.0).await
    }
}

// eth tx manager task

#[derive(Debug)]
struct EthTxManagerTask {
    pool: ConnectionPool,
    config: SenderConfig,
    gas_adjuster: Arc<dyn L1TxParamsProvider>,
    ethereum_gateway: Arc<dyn BoundEthInterface>,
    ethereum_gateway_blobs: Option<Arc<dyn BoundEthInterface>>,
}

#[async_trait::async_trait]
impl Task for EthTxManagerTask {
    fn name(&self) -> &'static str {
        "eth_tx_manager"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let eth_tx_manager_actor = EthTxManager::new(
            self.pool,
            self.config,
            self.gas_adjuster,
            self.ethereum_gateway,
            self.ethereum_gateway_blobs,
        );

        eth_tx_manager_actor.run(stop_receiver.0).await
    }
}

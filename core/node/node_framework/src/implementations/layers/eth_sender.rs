use std::sync::Arc;

use zksync_config::configs::eth_sender::SenderConfig;
use zksync_core::{
    eth_sender::{Aggregator, EthTxAggregator, EthTxManager},
    l1_gas_price::L1TxParamsProvider,
};
use zksync_dal::ConnectionPool;
use zksync_eth_client::BoundEthInterface;

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
    eth_sender_config: EthSenderConfig,
}

impl EthSenderLayer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthSenderLayer {
    fn layer_name(&self) -> &'static str {
        "eth_sender_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let eth_client_blobs_addr =
            PKSigningClient::from_config_blobs(&eth_sender, &contracts_config, &eth_client_config)
                .map(|k| k.sender_account());

        let eth_tx_aggregator_actor = EthTxAggregator::new(
            eth_sender.sender.clone(),
            Aggregator::new(
                eth_sender.sender.clone(),
                store_factory.create_store().await,
                eth_client_blobs_addr.is_some(),
                eth_sender.sender.pubdata_sending_mode.into(),
            ),
            Arc::new(eth_client),
            contracts_config.validator_timelock_addr,
            contracts_config.l1_multicall3_addr,
            main_zksync_contract_address,
            configs
                .network_config
                .as_ref()
                .context("network_config")?
                .zksync_network_id,
            eth_client_blobs_addr,
        )
        .await;
        task_futures.push(tokio::spawn(
            eth_tx_aggregator_actor.run(eth_sender_pool, stop_receiver.clone()),
        ));
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthTxAggregator].set(elapsed);
        tracing::info!("initialized ETH-TxAggregator in {elapsed:?}");

        let eth_manager_pool = ConnectionPool::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_manager_pool")?;
        let eth_sender = configs
            .eth_sender_config
            .clone()
            .context("eth_sender_config")?;
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let eth_client_blobs =
            PKSigningClient::from_config_blobs(&eth_sender, &contracts_config, &eth_client_config);
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

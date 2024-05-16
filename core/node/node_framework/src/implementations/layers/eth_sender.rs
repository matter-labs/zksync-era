use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{chain::NetworkConfig, eth_sender::EthConfig, ContractsConfig};
use zksync_eth_client::BoundEthInterface;
use zksync_eth_sender::{Aggregator, EthTxAggregator, EthTxManager};
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::{BoundEthInterfaceForBlobsResource, BoundEthInterfaceResource},
        l1_tx_params::L1TxParamsResource,
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct EthSenderLayer {
    eth_sender_config: EthConfig,
    contracts_config: ContractsConfig,
    network_config: NetworkConfig,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

impl EthSenderLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        network_config: NetworkConfig,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            network_config,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthSenderLayer {
    fn layer_name(&self) -> &'static str {
        "eth_sender_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await.unwrap();
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>().await?;
        let replica_pool = replica_pool_resource.get().await.unwrap();

        let eth_client = context.get_resource::<BoundEthInterfaceResource>().await?.0;
        let eth_client_blobs = match context
            .get_resource::<BoundEthInterfaceForBlobsResource>()
            .await
        {
            Ok(BoundEthInterfaceForBlobsResource(client)) => Some(client),
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(err) => return Err(err),
        };
        let object_store = context.get_resource::<ObjectStoreResource>().await?.0;

        // Create and add tasks.
        let eth_client_blobs_addr = eth_client_blobs
            .as_deref()
            .map(BoundEthInterface::sender_account);

        let config = self.eth_sender_config.sender.context("sender")?;
        let aggregator = Aggregator::new(
            config.clone(),
            object_store,
            eth_client_blobs_addr.is_some(),
            self.l1_batch_commit_data_generator_mode,
        );

        let eth_tx_aggregator_actor = EthTxAggregator::new(
            master_pool.clone(),
            config.clone(),
            aggregator,
            eth_client.clone(),
            self.contracts_config.validator_timelock_addr,
            self.contracts_config.l1_multicall3_addr,
            self.contracts_config.diamond_proxy_addr,
            self.network_config.zksync_network_id,
            eth_client_blobs_addr,
        )
        .await;

        context.add_task(Box::new(EthTxAggregatorTask {
            eth_tx_aggregator_actor,
        }));

        let gas_adjuster = context.get_resource::<L1TxParamsResource>().await?.0;

        let eth_tx_manager_actor = EthTxManager::new(
            master_pool,
            config,
            gas_adjuster,
            eth_client,
            eth_client_blobs,
        );

        context.add_task(Box::new(EthTxManagerTask {
            eth_tx_manager_actor,
        }));

        // Insert circuit breaker.
        let CircuitBreakersResource { breakers } = context.get_resource_or_default().await;
        breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        Ok(())
    }
}

#[derive(Debug)]
struct EthTxAggregatorTask {
    eth_tx_aggregator_actor: EthTxAggregator,
}

#[async_trait::async_trait]
impl Task for EthTxAggregatorTask {
    fn name(&self) -> &'static str {
        "eth_tx_aggregator"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.eth_tx_aggregator_actor.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct EthTxManagerTask {
    eth_tx_manager_actor: EthTxManager,
}

#[async_trait::async_trait]
impl Task for EthTxManagerTask {
    fn name(&self) -> &'static str {
        "eth_tx_manager"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.eth_tx_manager_actor.run(stop_receiver.0).await
    }
}

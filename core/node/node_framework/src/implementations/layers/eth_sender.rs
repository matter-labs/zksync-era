use std::sync::Arc;

use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{
    chain::{L1BatchCommitDataGeneratorMode, NetworkConfig},
    eth_sender::ETHConfig,
    wallets, ContractsConfig,
};
use zksync_core::eth_sender::{
    l1_batch_commit_data_generator::{
        L1BatchCommitDataGenerator, RollupModeL1BatchCommitDataGenerator,
        ValidiumModeL1BatchCommitDataGenerator,
    },
    Aggregator, EthTxAggregator, EthTxManager,
};
use zksync_eth_client::{clients::PKSigningClient, BoundEthInterface};
use zksync_types::L1ChainId;

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::BoundEthInterfaceResource,
        l1_tx_params::L1TxParamsResource,
        object_store::ObjectStoreResource,
        pools::{MasterPoolResource, ReplicaPoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct EthSenderLayer {
    eth_sender_config: ETHConfig,
    contracts_config: ContractsConfig,
    network_config: NetworkConfig,
    l1chain_id: L1ChainId,
    wallets: wallets::EthSender,
    l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
}

impl EthSenderLayer {
    pub fn new(
        eth_sender_config: ETHConfig,
        contracts_config: ContractsConfig,
        network_config: NetworkConfig,
        l1chain_id: L1ChainId,
        wallets: wallets::EthSender,
        l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            network_config,
            l1chain_id,
            wallets,
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
        let master_pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let master_pool = master_pool_resource.get().await.unwrap();

        let replica_pool_resource = context.get_resource::<ReplicaPoolResource>().await?;
        let replica_pool = replica_pool_resource.get().await.unwrap();

        let eth_client = context.get_resource::<BoundEthInterfaceResource>().await?.0;

        let object_store = context.get_resource::<ObjectStoreResource>().await?.0;

        // Create and add tasks.

        let eth_client_blobs = self.wallets.blob_operator.map(|wallet| {
            PKSigningClient::from_config(
                &self.eth_sender_config,
                &self.contracts_config,
                self.l1chain_id,
                wallet.private_key(),
            )
        });
        let eth_client_blobs_addr = eth_client_blobs.clone().map(|k| k.sender_account());

        let l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator> =
            match self.l1_batch_commit_data_generator_mode {
                L1BatchCommitDataGeneratorMode::Rollup => {
                    Arc::new(RollupModeL1BatchCommitDataGenerator {})
                }
                L1BatchCommitDataGeneratorMode::Validium => {
                    Arc::new(ValidiumModeL1BatchCommitDataGenerator {})
                }
            };

        let config = self.eth_sender_config.sender.context("sender")?;
        let aggregator = Aggregator::new(
            config.clone(),
            object_store,
            eth_client_blobs_addr.is_some(),
            l1_batch_commit_data_generator.clone(),
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
            l1_batch_commit_data_generator,
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
            eth_client_blobs.map(|c| Arc::new(c) as Arc<dyn BoundEthInterface>),
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

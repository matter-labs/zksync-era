use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{eth_sender::EthConfig, ContractsConfig};
use zksync_eth_client::BoundEthInterface;
use zksync_eth_sender::{Aggregator, EthTxAggregator, EthTxManager};
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::{BoundEthInterfaceForBlobsResource, BoundEthInterfaceResource},
        l1_tx_params::L1TxParamsResource,
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `eth_txs` managing
///
/// Responsible for initialization and running [`EthTxManager`] component, that manages sending
/// of `eth_txs`(such as `CommitBlocks`, `PublishProofBlocksOnchain` or `ExecuteBlock` ) to L1.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `PoolResource<ReplicaPool>`
/// - `BoundEthInterfaceResource`
/// - `BoundEthInterfaceForBlobsResource` (optional)
/// - `L1TxParamsResource`
/// - `CircuitBreakersResource` (adds a circuit breaker)
///
/// ## Adds tasks
///
/// - `EthTxManager`
#[derive(Debug)]
pub struct EthTxManagerLayer {
    eth_sender_config: EthConfig,
}

impl EthTxManagerLayer {
    pub fn new(eth_sender_config: EthConfig) -> Self {
        Self { eth_sender_config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthTxManagerLayer {
    fn layer_name(&self) -> &'static str {
        "eth_tx_manager_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let master_pool = master_pool_resource.get().await.unwrap();
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>()?;
        let replica_pool = replica_pool_resource.get().await.unwrap();

        let eth_client = context.get_resource::<BoundEthInterfaceResource>()?.0;
        let eth_client_blobs = match context.get_resource::<BoundEthInterfaceForBlobsResource>() {
            Ok(BoundEthInterfaceForBlobsResource(client)) => Some(client),
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(err) => return Err(err),
        };

        let config = self.eth_sender_config.sender.context("sender")?;

        let gas_adjuster = context.get_resource::<L1TxParamsResource>()?.0;

        let eth_tx_manager_actor = EthTxManager::new(
            master_pool,
            config,
            gas_adjuster,
            eth_client,
            eth_client_blobs,
        );

        context.add_task(eth_tx_manager_actor);

        // Insert circuit breaker.
        let CircuitBreakersResource { breakers } = context.get_resource_or_default();
        breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        Ok(())
    }
}

/// Wiring layer for aggregating l1 batches into `eth_txs`
///
/// Responsible for initialization and running of [`EthTxAggregator`], that aggregates L1 batches
/// into `eth_txs`(such as `CommitBlocks`, `PublishProofBlocksOnchain` or `ExecuteBlock`).
/// These `eth_txs` will be used as a queue for generating signed txs and will be sent later on L1.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `PoolResource<ReplicaPool>`
/// - `BoundEthInterfaceResource`
/// - `BoundEthInterfaceForBlobsResource` (optional)
/// - `ObjectStoreResource`
/// - `CircuitBreakersResource` (adds a circuit breaker)
///
/// ## Adds tasks
///
/// - `EthTxAggregator`
#[derive(Debug)]
pub struct EthTxAggregatorLayer {
    eth_sender_config: EthConfig,
    contracts_config: ContractsConfig,
    zksync_network_id: L2ChainId,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

impl EthTxAggregatorLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        zksync_network_id: L2ChainId,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            zksync_network_id,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthTxAggregatorLayer {
    fn layer_name(&self) -> &'static str {
        "eth_tx_aggregator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let master_pool = master_pool_resource.get().await.unwrap();
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>()?;
        let replica_pool = replica_pool_resource.get().await.unwrap();

        let eth_client = context.get_resource::<BoundEthInterfaceResource>()?.0;
        let eth_client_blobs = match context.get_resource::<BoundEthInterfaceForBlobsResource>() {
            Ok(BoundEthInterfaceForBlobsResource(client)) => Some(client),
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(err) => return Err(err),
        };
        let object_store = context.get_resource::<ObjectStoreResource>()?.0;

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
            self.zksync_network_id,
            eth_client_blobs_addr,
        )
        .await;

        context.add_task(eth_tx_aggregator_actor);

        // Insert circuit breaker.
        let CircuitBreakersResource { breakers } = context.get_resource_or_default();
        breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for EthTxAggregator {
    fn id(&self) -> TaskId {
        "eth_tx_aggregator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for EthTxManager {
    fn id(&self) -> TaskId {
        "eth_tx_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

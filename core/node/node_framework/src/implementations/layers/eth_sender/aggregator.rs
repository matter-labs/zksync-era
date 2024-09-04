use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{eth_sender::EthConfig, ContractsConfig};
use zksync_eth_client::BoundEthInterface;
use zksync_eth_sender::{Aggregator, EthTxAggregator};
use zksync_types::{commitment::L1BatchCommitmentMode, settlement::SettlementMode, L2ChainId};

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::{
            BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
            BoundEthInterfaceResource,
        },
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
        priority_merkle_tree::PriorityTreeResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

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
    gateway_contracts_config: Option<ContractsConfig>,
    zksync_network_id: L2ChainId,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    settlement_mode: SettlementMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub eth_client: Option<BoundEthInterfaceResource>,
    pub eth_client_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub eth_client_gateway: Option<BoundEthInterfaceForL2Resource>,
    pub object_store: ObjectStoreResource,
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
    pub priority_tree: PriorityTreeResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_tx_aggregator: EthTxAggregator,
}

impl EthTxAggregatorLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        gateway_contracts_config: Option<ContractsConfig>,
        zksync_network_id: L2ChainId,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
        settlement_mode: SettlementMode,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            gateway_contracts_config,
            zksync_network_id,
            l1_batch_commit_data_generator_mode,
            settlement_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthTxAggregatorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_tx_aggregator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Get resources.
        let contracts_config = if self.settlement_mode.is_gateway() {
            self.gateway_contracts_config.unwrap()
        } else {
            self.contracts_config
        };
        let eth_client = if self.settlement_mode.is_gateway() {
            input.eth_client_gateway.unwrap().0
        } else {
            input.eth_client.unwrap().0
        };
        let master_pool = input.master_pool.get().await.unwrap();
        let replica_pool = input.replica_pool.get().await.unwrap();

        let eth_client_blobs = input.eth_client_blobs.map(|c| c.0);
        let object_store = input.object_store.0;
        let priority_tree = input.priority_tree.0;

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
            priority_tree,
        );

        let eth_tx_aggregator = EthTxAggregator::new(
            master_pool.clone(),
            config.clone(),
            aggregator,
            eth_client,
            contracts_config.validator_timelock_addr,
            contracts_config.l1_multicall3_addr,
            contracts_config.diamond_proxy_addr,
            self.zksync_network_id,
            eth_client_blobs_addr,
            self.settlement_mode,
        )
        .await;

        // Insert circuit breaker.
        input
            .circuit_breakers
            .breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        Ok(Output { eth_tx_aggregator })
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

use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_eth_sender::{Aggregator, EthTxAggregator};
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        contracts::SettlementLayerContractsResource,
        eth_interface::{
            BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
            BoundEthInterfaceResource,
        },
        healthcheck::AppHealthCheckResource,
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
        settlement_layer::SettlementModeResource,
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
    zksync_network_id: L2ChainId,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
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
    pub settlement_mode: SettlementModeResource,
    pub sender_config: SenderConfig,
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
    pub contracts_resource: SettlementLayerContractsResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_tx_aggregator: EthTxAggregator,
}

impl EthTxAggregatorLayer {
    pub fn new(
        zksync_network_id: L2ChainId,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            zksync_network_id,
            l1_batch_commit_data_generator_mode,
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
        tracing::info!(
            "Wiring tx_aggregator in {:?} mode",
            input.settlement_mode.settlement_layer_for_sending_txs(),
        );
        tracing::info!("Contracts: {:?}", &input.contracts_resource.0);
        // Get resources.

        let validator_timelock_addr = input
            .contracts_resource
            .0
            .ecosystem_contracts
            .validator_timelock_addr
            .expect("Should be presented");
        let multicall3_addr = input
            .contracts_resource
            .0
            .ecosystem_contracts
            .multicall3
            .expect("Should be presented");
        let diamond_proxy_addr = input
            .contracts_resource
            .0
            .chain_contracts_config
            .diamond_proxy_addr;
        let state_transition_manager_address = input
            .contracts_resource
            .0
            .ecosystem_contracts
            .state_transition_proxy_addr
            .expect("Should be presented");

        let eth_client = if input.settlement_mode.settlement_layer().is_gateway() {
            input
                .eth_client_gateway
                .context("eth_client_gateway missing")?
                .0
        } else {
            input.eth_client.context("eth_client missing")?.0
        };

        let master_pool = input.master_pool.get().await.unwrap();
        let replica_pool = input.replica_pool.get().await.unwrap();

        let eth_client_blobs = input.eth_client_blobs.map(|c| c.0);
        let object_store = input.object_store.0;

        // Create and add tasks.
        let config = input.sender_config;
        let aggregator = Aggregator::new(
            config.clone(),
            object_store,
            eth_client_blobs.is_some(),
            self.l1_batch_commit_data_generator_mode,
            replica_pool.clone(),
            input.settlement_mode.settlement_layer(),
            config.precommit_params,
        )
        .await?;

        let eth_tx_aggregator = EthTxAggregator::new(
            master_pool.clone(),
            config.clone(),
            aggregator,
            eth_client,
            eth_client_blobs,
            validator_timelock_addr,
            state_transition_manager_address,
            multicall3_addr,
            diamond_proxy_addr,
            self.zksync_network_id,
            input.settlement_mode.settlement_layer_for_sending_txs(),
        )
        .await;

        // Insert circuit breaker.
        input
            .circuit_breakers
            .breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        input
            .app_health
            .0
            .insert_component(eth_tx_aggregator.health_check())
            .map_err(WiringError::internal)?;

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

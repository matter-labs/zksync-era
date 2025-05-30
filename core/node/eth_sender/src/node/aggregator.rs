use std::sync::Arc;

use anyhow::Context;
use zksync_circuit_breaker::{l1_txs::FailedL1TransactionChecker, CircuitBreakers};
use zksync_dal::node::{MasterPool, PoolResource, ReplicaPool};
use zksync_eth_client::{
    node::{
        contracts::SettlementLayerContractsResource, BoundEthInterfaceForBlobsResource,
        BoundEthInterfaceForL2Resource, SenderConfigResource,
    },
    web3_decl::node::SettlementModeResource,
    BoundEthInterface,
};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

use crate::{Aggregator, EthTxAggregator};

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
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    replica_pool: PoolResource<ReplicaPool>,
    eth_client: Option<Box<dyn BoundEthInterface>>,
    eth_client_blobs: Option<BoundEthInterfaceForBlobsResource>,
    eth_client_gateway: Option<BoundEthInterfaceForL2Resource>,
    object_store: Arc<dyn ObjectStore>,
    settlement_mode: SettlementModeResource,
    sender_config: SenderConfigResource,
    #[context(default)]
    circuit_breakers: Arc<CircuitBreakers>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
    sl_contracts: SettlementLayerContractsResource,
}

#[derive(Debug, IntoContext)]
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
        tracing::info!("Contracts: {:?}", input.sl_contracts);
        // Get resources.

        let ecosystem_contracts = &input.sl_contracts.0.ecosystem_contracts;
        let validator_timelock_addr = ecosystem_contracts
            .validator_timelock_addr
            .context("validator_timelock_addr not present in SL contracts")?;
        let multicall3_addr = ecosystem_contracts
            .multicall3
            .context("multicall3 not present in SL contracts")?;
        let state_transition_manager_address = ecosystem_contracts
            .state_transition_proxy_addr
            .context("state_transition_proxy_addr not present in SL contracts")?;
        let diamond_proxy_addr = input
            .sl_contracts
            .0
            .chain_contracts_config
            .diamond_proxy_addr;

        let eth_client = if input.settlement_mode.settlement_layer().is_gateway() {
            input
                .eth_client_gateway
                .context("eth_client_gateway missing")?
                .0
        } else {
            input.eth_client.context("eth_client missing")?
        };

        let master_pool = input.master_pool.get().await?;
        let replica_pool = input.replica_pool.get().await?;

        let eth_client_blobs = input.eth_client_blobs.map(|c| c.0);
        let object_store = input.object_store;

        // Create and add tasks.

        let config = input.sender_config.0;
        let aggregator = Aggregator::new(
            config.clone(),
            object_store,
            eth_client_blobs.is_some(),
            self.l1_batch_commit_data_generator_mode,
            replica_pool.clone(),
            input.settlement_mode.settlement_layer(),
            config.precommit_params.clone(),
        )
        .await?;

        let eth_tx_aggregator = EthTxAggregator::new(
            master_pool.clone(),
            config,
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
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        input
            .app_health
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

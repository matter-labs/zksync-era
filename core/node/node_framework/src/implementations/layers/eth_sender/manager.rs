use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::eth_sender::EthConfig;
use zksync_eth_sender::EthTxManager;

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::{
            BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
            BoundEthInterfaceResource,
        },
        gas_adjuster::GasAdjusterResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
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
/// - `TxParamsResource`
/// - `CircuitBreakersResource` (adds a circuit breaker)
///
/// ## Adds tasks
///
/// - `EthTxManager`
#[derive(Debug)]
pub struct EthTxManagerLayer {
    eth_sender_config: EthConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub eth_client: BoundEthInterfaceResource,
    pub eth_client_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub l2_client: Option<BoundEthInterfaceForL2Resource>,
    pub gas_adjuster: GasAdjusterResource,
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_tx_manager: EthTxManager,
}

impl EthTxManagerLayer {
    pub fn new(eth_sender_config: EthConfig) -> Self {
        Self { eth_sender_config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthTxManagerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_tx_manager_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Get resources.
        let master_pool = input.master_pool.get().await.unwrap();
        let replica_pool = input.replica_pool.get().await.unwrap();

        let eth_client = input.eth_client.0;
        let eth_client_blobs = input.eth_client_blobs.map(|c| c.0);
        let l2_client = input.l2_client.map(|c| c.0);

        let config = self.eth_sender_config.sender.context("sender")?;

        let gas_adjuster = input.gas_adjuster.0;

        let eth_tx_manager = EthTxManager::new(
            master_pool,
            config,
            gas_adjuster,
            Some(eth_client),
            eth_client_blobs,
            l2_client,
        );

        // Insert circuit breaker.
        input
            .circuit_breakers
            .breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        Ok(Output { eth_tx_manager })
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

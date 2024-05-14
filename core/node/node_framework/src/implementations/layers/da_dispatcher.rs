use std::sync::Arc;

use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{chain::L1BatchCommitDataGeneratorMode, eth_sender::EthConfig};
use zksync_eth_client::BoundEthInterface;
use zksync_eth_sender::{
    l1_batch_commit_data_generator::{
        L1BatchCommitDataGenerator, RollupModeL1BatchCommitDataGenerator,
        ValidiumModeL1BatchCommitDataGenerator,
    },
    Aggregator, EthTxAggregator, EthTxManager,
};

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        da_interface::DAInterfaceResource,
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
pub struct DataAvailabilityDispatcherLayer {
    eth_sender_config: EthConfig,
    l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
}

impl DataAvailabilityDispatcherLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    ) -> Self {
        Self {
            eth_sender_config,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityDispatcherLayer {
    fn layer_name(&self) -> &'static str {
        "da_dispatcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await.unwrap();

        let da_client = context.get_resource::<DAInterfaceResource>().await?.0;

        Ok(())
    }
}

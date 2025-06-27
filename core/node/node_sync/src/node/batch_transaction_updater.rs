use std::{num::NonZeroU64, sync::Arc, time::Duration};

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::node::SettlementLayerClient;

use crate::batch_transaction_updater::BatchTransactionUpdater;

#[derive(Debug, FromContext)]
pub struct Input {
    pool: PoolResource<MasterPool>,
    settlement_layer_client: SettlementLayerClient,
    sl_chain_contracts: SettlementLayerContractsResource,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    updater: BatchTransactionUpdater,
}

/// Wiring layer for `BatchTransactionUpdater`, part of the external node.
#[derive(Debug)]
pub struct BatchTransactionUpdaterLayer {
    sleep_interval: Duration,
    processing_batch_size: NonZeroU64,
}

impl BatchTransactionUpdaterLayer {
    pub fn new(sleep_interval: Duration, processing_batch_size: NonZeroU64) -> Self {
        Self {
            sleep_interval,
            processing_batch_size,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BatchTransactionUpdaterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "batch_transaction_updater_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            pool,
            settlement_layer_client,
            sl_chain_contracts,
            app_health,
        } = input;

        let updater = BatchTransactionUpdater::new(
            settlement_layer_client.into(),
            sl_chain_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr,
            pool.get().await?,
            self.sleep_interval,
            self.processing_batch_size,
        );

        // Insert healthcheck
        app_health
            .insert_component(updater.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { updater })
    }
}

#[async_trait::async_trait]
impl Task for BatchTransactionUpdater {
    fn id(&self) -> TaskId {
        "batch_transaction_updater".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

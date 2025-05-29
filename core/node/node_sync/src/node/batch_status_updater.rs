use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::EthInterface;
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::node::{MainNodeClientResource, SettlementLayerClient};

use crate::batch_status_updater::BatchStatusUpdater;

#[derive(Debug, FromContext)]
pub struct Input {
    pub pool: PoolResource<MasterPool>,
    pub client: MainNodeClientResource,
    pub settlement_layer_client: SettlementLayerClient,
    pub sl_chain_contracts: SettlementLayerContractsResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub updater: BatchStatusUpdater,
}

/// Wiring layer for `BatchStatusUpdater`, part of the external node.
#[derive(Debug)]
pub struct BatchStatusUpdaterLayer;

#[async_trait::async_trait]
impl WiringLayer for BatchStatusUpdaterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "batch_status_updater_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            pool,
            client,
            settlement_layer_client,
            sl_chain_contracts,
            app_health,
        } = input;

        let sl_client: Box<dyn EthInterface> = settlement_layer_client.into();

        let sl_chain_id = sl_client
            .fetch_chain_id()
            .await
            .map_err(WiringError::internal)?;

        let updater = BatchStatusUpdater::new(
            client.0,
            sl_client,
            sl_chain_contracts
                .0
                .chain_contracts_config
                .diamond_proxy_addr,
            pool.get().await?,
            sl_chain_id,
        );

        // Insert healthcheck
        app_health
            .0
            .insert_component(updater.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { updater })
    }
}

#[async_trait::async_trait]
impl Task for BatchStatusUpdater {
    fn id(&self) -> TaskId {
        "batch_status_updater".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

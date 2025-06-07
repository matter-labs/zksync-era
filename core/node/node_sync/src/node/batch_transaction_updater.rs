use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::EthInterface;
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::{
    client::{DynClient, L2},
    node::SettlementLayerClient,
};

use crate::batch_status_updater::BatchStatusUpdater;

#[derive(Debug, FromContext)]
pub struct Input {
    pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
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
pub struct BatchTransactionUpdaterLayer;

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
            main_node_client,
            settlement_layer_client,
            sl_chain_contracts,
            app_health,
        } = input;

        let sl_client: Box<dyn EthInterface> = settlement_layer_client.into();

        let sl_chain_id = sl_client
            .fetch_chain_id()
            .await
            .map_err(WiringError::internal)?;

        let updater = BatchTransactionUpdater::new(
            main_node_client,
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

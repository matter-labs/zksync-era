use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_eth_client::{node::contracts::SettlementLayerContractsResource, EthInterface};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    node::SettlementLayerClient,
};

use crate::tree_data_fetcher::TreeDataFetcher;

/// Wiring layer for [`TreeDataFetcher`].
#[derive(Debug)]
pub struct TreeDataFetcherLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: Box<DynClient<L2>>,
    pub gateway_client: SettlementLayerClient,
    pub settlement_layer_contracts_resource: SettlementLayerContractsResource,
    #[context(default)]
    pub app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: TreeDataFetcher,
}

#[async_trait::async_trait]
impl WiringLayer for TreeDataFetcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tree_data_fetcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let sl_client: Box<dyn EthInterface> = match input.gateway_client {
            SettlementLayerClient::L1(client) => Box::new(client),
            SettlementLayerClient::Gateway(client) => Box::new(client),
        };

        tracing::warn!(
            "Running tree data fetcher (allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up). \
             This is an experimental feature; do not use unless you know what you're doing"
        );
        let task = TreeDataFetcher::new(input.main_node_client, pool)
            .with_sl_data(
                sl_client,
                input
                    .settlement_layer_contracts_resource
                    .0
                    .chain_contracts_config
                    .diamond_proxy_addr,
            )
            .await?;

        // Insert healthcheck
        input
            .app_health
            .insert_component(task.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { task })
    }
}

#[async_trait::async_trait]
impl Task for TreeDataFetcher {
    fn id(&self) -> TaskId {
        "tree_data_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

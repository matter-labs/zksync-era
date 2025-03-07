use zksync_node_sync::tree_data_fetcher::TreeDataFetcher;

use crate::{
    implementations::resources::{
        contracts::SettlementLayerContractsResource,
        eth_interface::UniversalClientResource,
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`TreeDataFetcher`].
#[derive(Debug)]
pub struct TreeDataFetcherLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub gateway_client: UniversalClientResource,
    pub settlement_layer_contracts_resource: SettlementLayerContractsResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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
        let MainNodeClientResource(client) = input.main_node_client;
        let gateway_client = input.gateway_client.0;

        tracing::warn!(
            "Running tree data fetcher (allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up). \
             This is an experimental feature; do not use unless you know what you're doing"
        );
        let task = TreeDataFetcher::new(client, pool)
            .with_l1_data(
                gateway_client.into(),
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
            .0
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

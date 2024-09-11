use zksync_node_sync::tree_data_fetcher::TreeDataFetcher;
use zksync_types::Address;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
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
pub struct TreeDataFetcherLayer {
    diamond_proxy_addr: Address,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub eth_client: EthInterfaceResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: TreeDataFetcher,
}

impl TreeDataFetcherLayer {
    pub fn new(diamond_proxy_addr: Address) -> Self {
        Self { diamond_proxy_addr }
    }
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
        let EthInterfaceResource(eth_client) = input.eth_client;

        tracing::warn!(
            "Running tree data fetcher (allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up). \
             This is an experimental feature; do not use unless you know what you're doing"
        );
        let task =
            TreeDataFetcher::new(client, pool).with_l1_data(eth_client, self.diamond_proxy_addr)?;

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

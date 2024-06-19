use zksync_node_sync::tree_data_fetcher::TreeDataFetcher;
use zksync_types::Address;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource,
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct TreeDataFetcherLayer {
    diamond_proxy_addr: Address,
}

impl TreeDataFetcherLayer {
    pub fn new(diamond_proxy_addr: Address) -> Self {
        Self { diamond_proxy_addr }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TreeDataFetcherLayer {
    fn layer_name(&self) -> &'static str {
        "tree_data_fetcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context.get_resource::<PoolResource<MasterPool>>().await?;
        let MainNodeClientResource(client) = context.get_resource().await?;
        let EthInterfaceResource(eth_client) = context.get_resource().await?;

        tracing::warn!(
            "Running tree data fetcher (allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up). \
             This is an experimental feature; do not use unless you know what you're doing"
        );
        let fetcher = TreeDataFetcher::new(client, pool.get().await?)
            .with_l1_data(eth_client, self.diamond_proxy_addr)?;

        // Insert healthcheck
        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health
            .insert_component(fetcher.health_check())
            .map_err(WiringError::internal)?;

        // Insert task
        context.add_task(Box::new(fetcher));

        Ok(())
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

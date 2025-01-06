use zksync_node_sync::data_availability_fetcher::DataAvailabilityFetcher;
use zksync_types::{Address, L2ChainId};

use crate::{
    implementations::resources::{
        eth_interface::{EthInterfaceResource, GatewayEthInterfaceResource},
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`DataAvailabilityFetcher`].
#[derive(Debug)]
pub struct DataAvailabilityFetcherLayer {
    l1_diamond_proxy_addr: Address,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub l1_client: EthInterfaceResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: DataAvailabilityFetcher,
}

impl DataAvailabilityFetcherLayer {
    pub fn new(l1_diamond_proxy_addr: Address, l2_chain_id: L2ChainId) -> Self {
        Self {
            l1_diamond_proxy_addr,
            l2_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityFetcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "data_availability_fetcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let MainNodeClientResource(client) = input.main_node_client;
        let EthInterfaceResource(l1_client) = input.l1_client;

        tracing::info!("Running data availability fetcher.");
        let task = DataAvailabilityFetcher::new(client, pool);

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
impl Task for DataAvailabilityFetcher {
    fn id(&self) -> TaskId {
        "data_availability_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

use zksync_da_client::DataAvailabilityClient;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::node::MainNodeClientResource;

use crate::data_availability_fetcher::DataAvailabilityFetcher;

/// Wiring layer for [`DataAvailabilityFetcher`].
#[derive(Debug)]
pub struct DataAvailabilityFetcherLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    main_node_client: MainNodeClientResource,
    da_client: Box<dyn DataAvailabilityClient>,
    #[context(default)]
    app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    task: DataAvailabilityFetcher,
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

        tracing::info!("Running data availability fetcher.");
        let task = DataAvailabilityFetcher::new(client, pool, input.da_client);

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

use std::sync::Arc;

use zksync_da_client::DataAvailabilityClient;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::client::{DynClient, L2};

use crate::data_availability_fetcher::DataAvailabilityFetcher;

/// Wiring layer for [`DataAvailabilityFetcher`].
#[derive(Debug)]
pub struct DataAvailabilityFetcherLayer {
    max_batches_to_recheck: u32,
}

impl DataAvailabilityFetcherLayer {
    pub fn new(max_batches_to_recheck: u32) -> Self {
        Self {
            max_batches_to_recheck,
        }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
    da_client: Box<dyn DataAvailabilityClient>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
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

        tracing::info!("Running data availability fetcher.");
        let task = DataAvailabilityFetcher::new(input.main_node_client, pool, input.da_client, self.max_batches_to_recheck);

        // Insert healthcheck
        input
            .app_health
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

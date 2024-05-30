use zksync_config::configs::da_dispatcher::DADispatcherConfig;
use zksync_da_layers::DataAvailabilityClient;
use zksync_dal::Core;
use zksync_db_connection::connection_pool::ConnectionPool;

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// A layer that wires the data availability dispatcher task.
#[derive(Debug)]
pub struct DataAvailabilityDispatcherLayer {
    da_config: DADispatcherConfig,
}

impl DataAvailabilityDispatcherLayer {
    pub fn new(da_config: DADispatcherConfig) -> Self {
        Self { da_config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityDispatcherLayer {
    fn layer_name(&self) -> &'static str {
        "da_dispatcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await?;
        let da_client = context.get_resource::<DAClientResource>().await?.0;

        context.add_task(Box::new(DataAvailabilityDispatcherTask {
            main_pool: master_pool,
            da_config: self.da_config,
            client: da_client,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct DataAvailabilityDispatcherTask {
    main_pool: ConnectionPool<Core>,
    da_config: DADispatcherConfig,
    client: Box<dyn DataAvailabilityClient>,
}

#[async_trait::async_trait]
impl Task for DataAvailabilityDispatcherTask {
    fn name(&self) -> &'static str {
        "da_dispatcher"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let da_dispatcher = zksync_da_dispatcher::DataAvailabilityDispatcher::new(
            self.main_pool,
            self.da_config,
            self.client,
        );

        da_dispatcher.run(stop_receiver.0).await
    }
}

use zksync_config::configs::{chain::StateKeeperConfig, da_dispatcher::DADispatcherConfig};
use zksync_da_dispatcher::DataAvailabilityDispatcher;

use crate::{
    implementations::resources::{
        da_client::DAClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// A layer that wires the data availability dispatcher task.
#[derive(Debug)]
pub struct DataAvailabilityDispatcherLayer {
    state_keeper_config: StateKeeperConfig,
    da_config: DADispatcherConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub da_client: DAClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub da_dispatcher_task: DataAvailabilityDispatcher,
}

impl DataAvailabilityDispatcherLayer {
    pub fn new(state_keeper_config: StateKeeperConfig, da_config: DADispatcherConfig) -> Self {
        Self {
            state_keeper_config,
            da_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityDispatcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "da_dispatcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // A pool with size 2 is used here because there are 2 functions within a task that execute in parallel
        let master_pool = input.master_pool.get_custom(2).await?;
        let da_client = input.da_client.0;

        if let Some(limit) = da_client.blob_size_limit() {
            if self.state_keeper_config.max_pubdata_per_batch > limit as u64 {
                return Err(WiringError::Configuration(format!(
                    "Max pubdata per batch is greater than the blob size limit: {} > {}",
                    self.state_keeper_config.max_pubdata_per_batch, limit
                )));
            }
        }

        let da_dispatcher_task =
            DataAvailabilityDispatcher::new(master_pool, self.da_config, da_client);

        Ok(Output { da_dispatcher_task })
    }
}

#[async_trait::async_trait]
impl Task for DataAvailabilityDispatcher {
    fn id(&self) -> TaskId {
        "da_dispatcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

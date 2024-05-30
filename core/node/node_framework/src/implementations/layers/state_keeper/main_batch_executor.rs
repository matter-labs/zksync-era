use zksync_config::configs::chain::StateKeeperConfig;
use zksync_state_keeper::MainBatchExecutor;

use crate::{
    implementations::resources::state_keeper::BatchExecutorResource,
    resource::Unique,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    state_keeper_config: StateKeeperConfig,
}

impl MainBatchExecutorLayer {
    pub fn new(state_keeper_config: StateKeeperConfig) -> Self {
        Self {
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainBatchExecutorLayer {
    fn layer_name(&self) -> &'static str {
        "main_batch_executor_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let builder = MainBatchExecutor::new(self.state_keeper_config.save_call_traces, false);

        context.insert_resource(BatchExecutorResource(Unique::new(Box::new(builder))))?;
        Ok(())
    }
}

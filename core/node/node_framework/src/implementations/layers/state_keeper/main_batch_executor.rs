use zksync_state_keeper::MainBatchExecutor;

use crate::{
    implementations::resources::state_keeper::BatchExecutorResource,
    resource::Unique,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    save_call_traces: bool,
}

impl MainBatchExecutorLayer {
    pub fn new(save_call_traces: bool) -> Self {
        Self { save_call_traces }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainBatchExecutorLayer {
    fn layer_name(&self) -> &'static str {
        "main_batch_executor_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let builder = MainBatchExecutor::new(self.save_call_traces, false);

        context.insert_resource(BatchExecutorResource(Unique::new(Box::new(builder))))?;
        Ok(())
    }
}

use zksync_state_keeper::MainBatchExecutor;

use crate::{
    implementations::resources::state_keeper::BatchExecutorResource,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `MainBatchExecutor`, part of the state keeper responsible for running the VM.
#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
}

impl MainBatchExecutorLayer {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainBatchExecutorLayer {
    type Input = ();
    type Output = BatchExecutorResource;

    fn layer_name(&self) -> &'static str {
        "main_batch_executor_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let executor =
            MainBatchExecutor::new(self.save_call_traces, self.optional_bytecode_compression);
        Ok(executor.into())
    }
}

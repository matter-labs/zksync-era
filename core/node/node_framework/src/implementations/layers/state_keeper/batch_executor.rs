use zksync_state_keeper::BatchExecutor;

use crate::{
    implementations::resources::state_keeper::BatchExecutorResource,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BatchExecutor`, part of the state keeper responsible for running the VM.
#[derive(Debug)]
pub struct BatchExecutorLayer {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
}

impl BatchExecutorLayer {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BatchExecutorLayer {
    type Input = ();
    type Output = BatchExecutorResource;

    fn layer_name(&self) -> &'static str {
        "batch_executor_layer"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let batch_executor =
            BatchExecutor::new(self.save_call_traces, self.optional_bytecode_compression);
        Ok(batch_executor.into())
    }
}

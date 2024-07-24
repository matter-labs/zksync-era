use zksync_state_keeper::{FastVmMode, MainBatchExecutor};

use crate::{
    implementations::resources::state_keeper::BatchExecutorResource,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `MainBatchExecutor`, part of the state keeper responsible for running the VM.
#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
    fast_vm_mode: Option<FastVmMode>,
}

impl MainBatchExecutorLayer {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
            fast_vm_mode: None,
        }
    }

    pub fn set_fast_vm_mode(&mut self, fast_vm_mode: Option<FastVmMode>) {
        self.fast_vm_mode = fast_vm_mode;
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
        let mut executor =
            MainBatchExecutor::new(self.save_call_traces, self.optional_bytecode_compression);
        executor.set_fast_vm_mode(self.fast_vm_mode);
        Ok(executor.into())
    }
}

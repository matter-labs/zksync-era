use zksync_node_framework::wiring_layer::{WiringError, WiringLayer};
use zksync_types::vm::FastVmMode;
use zksync_vm_executor::batch::{BatchTracer, MainBatchExecutorFactory, TraceCalls};

use super::resources::BatchExecutorResource;

/// Wiring layer for `MainBatchExecutor`, part of the state keeper responsible for running the VM.
#[derive(Debug)]
pub struct MainBatchExecutorLayer {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
}

impl MainBatchExecutorLayer {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
            fast_vm_mode: FastVmMode::default(),
        }
    }

    pub fn with_fast_vm_mode(mut self, mode: FastVmMode) -> Self {
        self.fast_vm_mode = mode;
        self
    }

    fn create_executor<Tr: BatchTracer>(&self) -> BatchExecutorResource {
        let mut executor = MainBatchExecutorFactory::<Tr>::new(self.optional_bytecode_compression);
        executor.set_fast_vm_mode(self.fast_vm_mode);
        executor.into()
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
        Ok(if self.save_call_traces {
            self.create_executor::<TraceCalls>()
        } else {
            self.create_executor::<()>()
        })
    }
}

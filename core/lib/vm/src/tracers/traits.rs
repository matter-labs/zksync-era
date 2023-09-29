use zk_evm::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::bootloader_state::BootloaderState;
use crate::old_vm::memory::SimpleMemory;
use crate::types::internals::ZkSyncVmState;
use crate::types::outputs::VmExecutionResultAndLogs;
use crate::VmExecutionStopReason;

/// Run tracer for collecting data during the vm execution cycles
pub trait ExecutionProcessing<S: WriteStorage>: DynTracer<S> + ExecutionEndTracer {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S>) {}
    fn before_cycle(&mut self, _state: &mut ZkSyncVmState<S>) {}
    fn after_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S>,
        _bootloader_state: &mut BootloaderState,
    ) {
    }
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
    }
}

/// Stop the vm execution if the tracer conditions are met
pub trait ExecutionEndTracer {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool {
        false
    }
}

/// Version of zk_evm::Tracer suitable for dynamic dispatch.
pub trait DynTracer<S> {
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &SimpleMemory) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &SimpleMemory,
    ) {
    }
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory,
        _storage: StoragePtr<S>,
    ) {
    }
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory,
        _storage: StoragePtr<S>,
    ) {
    }
}

/// Save the results of the vm execution.
pub trait VmTracer<S: WriteStorage>:
    DynTracer<S> + ExecutionEndTracer + ExecutionProcessing<S> + Send
{
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {}
}

pub trait BoxedTracer<S> {
    fn into_boxed(self) -> Box<dyn VmTracer<S>>;
}

impl<S: WriteStorage, T: VmTracer<S> + 'static> BoxedTracer<S> for T {
    fn into_boxed(self) -> Box<dyn VmTracer<S>> {
        Box::new(self)
    }
}

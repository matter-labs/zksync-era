use zk_evm::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::bootloader_state::BootloaderState;
use crate::old_vm::history_recorder::HistoryMode;
use crate::old_vm::memory::SimpleMemory;
use crate::types::internals::ZkSyncVmState;
use crate::types::outputs::VmExecutionResultAndLogs;
use crate::{Halt, VmExecutionStopReason};

/// Run tracer for collecting data during the vm execution cycles
pub trait ExecutionProcessing<S: WriteStorage, H: HistoryMode>:
    DynTracer<S, H> + ExecutionEndTracer<H>
{
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    fn before_cycle(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    fn after_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) {
    }
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
    }
}

/// Version of zk_evm::Tracer suitable for dynamic dispatch.
pub trait DynTracer<S, H: HistoryMode> {
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &SimpleMemory<H>) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &SimpleMemory<H>,
    ) {
    }
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
    }
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
    }
}

/// Save the results of the vm execution.
pub trait VmTracer<S: WriteStorage, H: HistoryMode>:
    DynTracer<S, H> + ExecutionEndTracer<H> + ExecutionProcessing<S, H> + Send
{
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {}
}

pub trait BoxedTracer<S, H> {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>>;
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H> + 'static> BoxedTracer<S, H> for T {
    fn into_boxed(self) -> Box<dyn VmTracer<S, H>> {
        Box::new(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStopReason {
    Finish,
    Abort(Halt),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStatus {
    Continue,
    Stop(TracerExecutionStopReason),
}

/// Stop the vm execution if the tracer conditions are met
pub trait ExecutionEndTracer<H: HistoryMode> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> TracerExecutionStatus {
        TracerExecutionStatus::Continue
    }
}

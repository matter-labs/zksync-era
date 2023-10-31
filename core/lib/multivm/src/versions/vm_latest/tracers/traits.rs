use crate::interface::Halt;
use zk_evm_1_3_3::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::vm_latest::bootloader_state::BootloaderState;
use crate::vm_latest::old_vm::history_recorder::HistoryMode;
use crate::vm_latest::old_vm::memory::SimpleMemory;
use crate::vm_latest::types::internals::ZkSyncVmState;
use crate::vm_latest::VmExecutionStopReason;

/// Run tracer for collecting data during the vm execution cycles
pub trait VmTracer<S: WriteStorage, H: HistoryMode>: DynTracer<S, H> {
    /// Initialize the tracer before the vm execution
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {}
    /// Run after each vm execution cycle
    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        TracerExecutionStatus::Continue
    }
    /// Run after the vm execution
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
    }
}

/// Version of zk_evm_1_3_3::Tracer suitable for dynamic dispatch.
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

impl TracerExecutionStatus {
    /// Chose the stricter ExecutionStatus
    /// If both statuses are Continue, then the result is Continue
    /// If one of the statuses is Abort, then the result is Abort
    /// If one of the statuses is Finish, then the result is Finish
    pub fn stricter(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Continue, Self::Continue) => Self::Continue,
            (Self::Stop(TracerExecutionStopReason::Abort(reason)), _)
            | (_, Self::Stop(TracerExecutionStopReason::Abort(reason))) => {
                Self::Stop(TracerExecutionStopReason::Abort(reason.clone()))
            }
            (Self::Stop(TracerExecutionStopReason::Finish), _)
            | (_, Self::Stop(TracerExecutionStopReason::Finish)) => {
                Self::Stop(TracerExecutionStopReason::Finish)
            }
        }
    }
}

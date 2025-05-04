use zk_evm_1_5_2::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStatus, VmExecutionStopReason},
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{
        bootloader::BootloaderState, HistoryMode, SimpleMemory, TracerPointer, VmTracer,
        ZkSyncVmState,
    },
};

/// Tracer dispatcher is a tracer that can dispatch calls to multiple tracers.
pub struct TracerDispatcher<S: WriteStorage, H: HistoryMode> {
    tracers: Vec<TracerPointer<S, H>>,
}

impl<S: WriteStorage, H: HistoryMode> TracerDispatcher<S, H> {
    pub fn new(tracers: Vec<TracerPointer<S, H>>) -> Self {
        Self { tracers }
    }
}

impl<S: WriteStorage, H: HistoryMode> From<TracerPointer<S, H>> for TracerDispatcher<S, H> {
    fn from(value: TracerPointer<S, H>) -> Self {
        Self {
            tracers: vec![value],
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> From<Vec<TracerPointer<S, H>>> for TracerDispatcher<S, H> {
    fn from(value: Vec<TracerPointer<S, H>>) -> Self {
        Self { tracers: value }
    }
}

impl<S: WriteStorage, H: HistoryMode> Default for TracerDispatcher<S, H> {
    fn default() -> Self {
        Self { tracers: vec![] }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for TracerDispatcher<S, H> {
    #[inline(always)]
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &SimpleMemory<H>) {
        for tracer in self.tracers.iter_mut() {
            tracer.before_decoding(_state, _memory);
        }
    }

    #[inline(always)]
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &SimpleMemory<H>,
    ) {
        for tracer in self.tracers.iter_mut() {
            tracer.after_decoding(_state, _data, _memory);
        }
    }

    #[inline(always)]
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        for tracer in self.tracers.iter_mut() {
            tracer.before_execution(_state, _data, _memory, _storage.clone());
        }
    }

    #[inline(always)]
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        for tracer in self.tracers.iter_mut() {
            tracer.after_execution(_state, _data, _memory, _storage.clone());
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for TracerDispatcher<S, H> {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {
        for tracer in self.tracers.iter_mut() {
            tracer.initialize_tracer(_state);
        }
    }

    /// Run after each vm execution cycle
    #[inline(always)]
    fn finish_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        let mut result = TracerExecutionStatus::Continue;
        for tracer in self.tracers.iter_mut() {
            result = result.stricter(&tracer.finish_cycle(_state, _bootloader_state));
        }
        result
    }

    /// Run after the vm execution
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
        for tracer in self.tracers.iter_mut() {
            tracer.after_vm_execution(_state, _bootloader_state, _stop_reason.clone());
        }
    }
}

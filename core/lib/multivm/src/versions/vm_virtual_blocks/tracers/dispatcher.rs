use zk_evm_1_3_3::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};

use crate::{
    interface::{
        dyn_tracers::vm_1_3_3::DynTracer,
        storage::{StoragePtr, WriteStorage},
        tracer::VmExecutionStopReason,
        VmExecutionResultAndLogs,
    },
    vm_virtual_blocks::{
        BootloaderState, ExecutionEndTracer, ExecutionProcessing, HistoryMode, SimpleMemory,
        TracerPointer, VmTracer, ZkSyncVmState,
    },
};

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

pub struct TracerDispatcher<S: WriteStorage, H: HistoryMode> {
    tracers: Vec<TracerPointer<S, H>>,
}

impl<S: WriteStorage, H: HistoryMode> TracerDispatcher<S, H> {
    pub fn new(tracers: Vec<TracerPointer<S, H>>) -> Self {
        Self { tracers }
    }
}

impl<S: WriteStorage, H: HistoryMode> Default for TracerDispatcher<S, H> {
    fn default() -> Self {
        Self { tracers: vec![] }
    }
}
impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for TracerDispatcher<S, H> {
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &SimpleMemory<H>) {
        for tracer in self.tracers.iter_mut() {
            tracer.before_decoding(_state, _memory);
        }
    }

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

impl<S: WriteStorage, H: HistoryMode> ExecutionEndTracer<H> for TracerDispatcher<S, H> {
    fn should_stop_execution(&self) -> bool {
        let mut result = false;
        for tracer in self.tracers.iter() {
            result |= tracer.should_stop_execution();
        }
        result
    }
}

impl<S: WriteStorage, H: HistoryMode> ExecutionProcessing<S, H> for TracerDispatcher<S, H> {
    fn initialize_tracer(&mut self, _state: &mut ZkSyncVmState<S, H>) {
        for tracer in self.tracers.iter_mut() {
            tracer.initialize_tracer(_state);
        }
    }

    /// Run after each vm execution cycle
    fn after_cycle(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) {
        for tracer in self.tracers.iter_mut() {
            tracer.after_cycle(_state, _bootloader_state);
        }
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

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for TracerDispatcher<S, H> {
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {
        for tracer in self.tracers.iter_mut() {
            tracer.save_results(_result);
        }
    }
}

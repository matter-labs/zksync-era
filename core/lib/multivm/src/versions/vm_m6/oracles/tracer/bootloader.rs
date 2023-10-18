use std::marker::PhantomData;

use crate::vm_m6::history_recorder::HistoryMode;
use crate::vm_m6::memory::SimpleMemory;
use crate::vm_m6::oracles::tracer::{
    utils::gas_spent_on_bytecodes_and_long_messages_this_opcode, ExecutionEndTracer,
    PendingRefundTracer, PubdataSpentTracer, StorageInvocationTracer,
};

use zk_evm_1_3_1::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::{ErrorFlags, VmLocalState},
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{Opcode, RetOpcode},
};

/// Tells the VM to end the execution before `ret` from the booloader if there is no panic or revert.
/// Also, saves the information if this `ret` was caused by "out of gas" panic.
#[derive(Debug, Clone, Default)]
pub struct BootloaderTracer<H: HistoryMode> {
    is_bootloader_out_of_gas: bool,
    ret_from_the_bootloader: Option<RetOpcode>,
    gas_spent_on_bytecodes_and_long_messages: u32,
    _marker: PhantomData<H>,
}

impl<H: HistoryMode> Tracer for BootloaderTracer<H> {
    const CALL_AFTER_DECODING: bool = true;
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory<H>;

    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}
    fn after_decoding(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        _memory: &Self::SupportedMemory,
    ) {
        // We should check not only for the `NOT_ENOUGH_ERGS` flag but if the current frame is bootloader too.
        if Self::current_frame_is_bootloader(state.vm_local_state)
            && data
                .error_flags_accumulated
                .contains(ErrorFlags::NOT_ENOUGH_ERGS)
        {
            self.is_bootloader_out_of_gas = true;
        }
    }

    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &Self::SupportedMemory,
    ) {
        self.gas_spent_on_bytecodes_and_long_messages +=
            gas_spent_on_bytecodes_and_long_messages_this_opcode(&state, &data);
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        // Decodes next opcode.
        // `self` is passed as `tracer`, so `self.after_decoding` will be called and it will catch "out of gas".
        let (next_opcode, _, _) = zk_evm_1_3_1::vm_state::read_and_decode(
            state.vm_local_state,
            memory,
            &mut DummyTracer,
            self,
        );
        if Self::current_frame_is_bootloader(state.vm_local_state) {
            if let Opcode::Ret(ret) = next_opcode.inner.variant.opcode {
                self.ret_from_the_bootloader = Some(ret);
            }
        }
    }
}

impl<H: HistoryMode> ExecutionEndTracer<H> for BootloaderTracer<H> {
    fn should_stop_execution(&self) -> bool {
        self.ret_from_the_bootloader == Some(RetOpcode::Ok)
    }
}

impl<H: HistoryMode> PendingRefundTracer<H> for BootloaderTracer<H> {}
impl<H: HistoryMode> StorageInvocationTracer<H> for BootloaderTracer<H> {}

impl<H: HistoryMode> PubdataSpentTracer<H> for BootloaderTracer<H> {
    fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }
}

impl<H: HistoryMode> BootloaderTracer<H> {
    fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
        // The current frame is bootloader if the callstack depth is 1.
        // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
        // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
        local_state.callstack.inner.len() == 1
    }

    pub fn is_bootloader_out_of_gas(&self) -> bool {
        self.is_bootloader_out_of_gas
    }

    pub fn bootloader_panicked(&self) -> bool {
        self.ret_from_the_bootloader == Some(RetOpcode::Panic)
    }
}

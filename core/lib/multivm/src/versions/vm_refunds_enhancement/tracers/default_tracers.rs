use std::fmt::{Debug, Formatter};

use zk_evm_1_3_3::{
    aux_structures::Timestamp,
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{decoding::EncodingModeProduction, Opcode, RetOpcode},
};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStatus, TracerExecutionStopReason, VmExecutionStopReason},
        Halt, VmExecutionMode,
    },
    tracers::dyn_tracers::vm_1_3_3::DynTracer,
    vm_refunds_enhancement::{
        bootloader_state::{utils::apply_l2_block, BootloaderState},
        constants::BOOTLOADER_HEAP_PAGE,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::{
            dispatcher::TracerDispatcher,
            utils::{
                computational_gas_price, gas_spent_on_bytecodes_and_long_messages_this_opcode,
                print_debug_if_needed, VmHook,
            },
            RefundsTracer, ResultTracer,
        },
        types::internals::ZkSyncVmState,
        VmTracer,
    },
};

/// Default tracer for the VM. It manages the other tracers execution and stop the vm when needed.
pub(crate) struct DefaultExecutionTracer<S: WriteStorage, H: HistoryMode> {
    tx_has_been_processed: bool,
    execution_mode: VmExecutionMode,

    pub(crate) gas_spent_on_bytecodes_and_long_messages: u32,
    // Amount of gas used during account validation.
    pub(crate) computational_gas_used: u32,
    // Maximum number of gas that we're allowed to use during account validation.
    tx_validation_gas_limit: u32,
    in_account_validation: bool,
    final_batch_info_requested: bool,
    pub(crate) result_tracer: ResultTracer,
    // This tracer is designed specifically for calculating refunds. Its separation from the custom tracer
    // ensures static dispatch, enhancing performance by avoiding dynamic dispatch overhead.
    // Additionally, being an internal tracer, it saves the results directly to `VmResultAndLogs`.
    pub(crate) refund_tracer: Option<RefundsTracer>,
    pub(crate) dispatcher: TracerDispatcher<S, H>,
    ret_from_the_bootloader: Option<RetOpcode>,
    storage: StoragePtr<S>,
}

impl<S: WriteStorage, H: HistoryMode> Debug for DefaultExecutionTracer<S, H> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExecutionTracer").finish()
    }
}

impl<S: WriteStorage, H: HistoryMode> Tracer for DefaultExecutionTracer<S, H> {
    const CALL_BEFORE_DECODING: bool = false;
    const CALL_AFTER_DECODING: bool = true;
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory<H>;

    fn before_decoding(
        &mut self,
        _state: VmLocalStateData<'_, 8, EncodingModeProduction>,
        _memory: &Self::SupportedMemory,
    ) {
    }

    fn after_decoding(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        memory: &Self::SupportedMemory,
    ) {
        <ResultTracer as DynTracer<S, SimpleMemory<H>>>::after_decoding(
            &mut self.result_tracer,
            state,
            data,
            memory,
        );

        if let Some(refund_tracer) = &mut self.refund_tracer {
            <RefundsTracer as DynTracer<S, SimpleMemory<H>>>::after_decoding(
                refund_tracer,
                state,
                data,
                memory,
            );
        }

        self.dispatcher.after_decoding(state, data, memory);
    }

    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        if self.in_account_validation {
            self.computational_gas_used = self
                .computational_gas_used
                .saturating_add(computational_gas_price(state, &data));
        }

        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        match hook {
            VmHook::TxHasEnded => self.tx_has_been_processed = true,
            VmHook::NoValidationEntered => self.in_account_validation = false,
            VmHook::AccountValidationEntered => self.in_account_validation = true,
            VmHook::FinalBatchInfo => self.final_batch_info_requested = true,
            _ => {}
        }

        self.gas_spent_on_bytecodes_and_long_messages +=
            gas_spent_on_bytecodes_and_long_messages_this_opcode(&state, &data);
        self.result_tracer
            .before_execution(state, data, memory, self.storage.clone());

        if let Some(refund_tracer) = &mut self.refund_tracer {
            refund_tracer.before_execution(state, data, memory, self.storage.clone());
        }
        self.dispatcher
            .before_execution(state, data, memory, self.storage.clone());
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        if let VmExecutionMode::Bootloader = self.execution_mode {
            let (next_opcode, _, _) = zk_evm_1_3_3::vm_state::read_and_decode(
                state.vm_local_state,
                memory,
                &mut DummyTracer,
                self,
            );
            if current_frame_is_bootloader(state.vm_local_state) {
                if let Opcode::Ret(ret) = next_opcode.inner.variant.opcode {
                    self.ret_from_the_bootloader = Some(ret);
                }
            }
        }

        self.result_tracer
            .after_execution(state, data, memory, self.storage.clone());
        if let Some(refund_tracer) = &mut self.refund_tracer {
            refund_tracer.after_execution(state, data, memory, self.storage.clone())
        }
        self.dispatcher
            .after_execution(state, data, memory, self.storage.clone());
    }
}

impl<S: WriteStorage, H: HistoryMode> DefaultExecutionTracer<S, H> {
    pub(crate) fn new(
        computational_gas_limit: u32,
        execution_mode: VmExecutionMode,
        dispatcher: TracerDispatcher<S, H>,
        storage: StoragePtr<S>,
        refund_tracer: Option<RefundsTracer>,
    ) -> Self {
        Self {
            tx_has_been_processed: false,
            execution_mode,
            gas_spent_on_bytecodes_and_long_messages: 0,
            computational_gas_used: 0,
            tx_validation_gas_limit: computational_gas_limit,
            in_account_validation: false,
            final_batch_info_requested: false,
            result_tracer: ResultTracer::new(execution_mode),
            refund_tracer,
            dispatcher,
            ret_from_the_bootloader: None,
            storage,
        }
    }

    pub(crate) fn tx_has_been_processed(&self) -> bool {
        self.tx_has_been_processed
    }

    pub(crate) fn validation_run_out_of_gas(&self) -> bool {
        self.computational_gas_used > self.tx_validation_gas_limit
    }

    pub(crate) fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }

    fn set_fictive_l2_block(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) {
        let current_timestamp = Timestamp(state.local_state.timestamp);
        let txs_index = bootloader_state.free_tx_index();
        let l2_block = bootloader_state.insert_fictive_l2_block();
        let mut memory = vec![];
        apply_l2_block(&mut memory, l2_block, txs_index);
        state
            .memory
            .populate_page(BOOTLOADER_HEAP_PAGE as usize, memory, current_timestamp);
        self.final_batch_info_requested = false;
    }

    fn should_stop_execution(&self) -> TracerExecutionStatus {
        match self.execution_mode {
            VmExecutionMode::OneTx if self.tx_has_been_processed() => {
                return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
            }
            VmExecutionMode::Bootloader if self.ret_from_the_bootloader == Some(RetOpcode::Ok) => {
                return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
            }
            _ => {}
        };
        if self.validation_run_out_of_gas() {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                Halt::ValidationOutOfGas,
            ));
        }
        TracerExecutionStatus::Continue
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>>
    for DefaultExecutionTracer<S, H>
{
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for DefaultExecutionTracer<S, H> {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.result_tracer.initialize_tracer(state);
        if let Some(refund_tracer) = &mut self.refund_tracer {
            refund_tracer.initialize_tracer(state);
        }
        self.dispatcher.initialize_tracer(state);
    }

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        if self.final_batch_info_requested {
            self.set_fictive_l2_block(state, bootloader_state)
        }

        let mut result = self.result_tracer.finish_cycle(state, bootloader_state);
        if let Some(refund_tracer) = &mut self.refund_tracer {
            result = refund_tracer
                .finish_cycle(state, bootloader_state)
                .stricter(&result);
        }
        result = self
            .dispatcher
            .finish_cycle(state, bootloader_state)
            .stricter(&result);
        result.stricter(&self.should_stop_execution())
    }

    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        stop_reason: VmExecutionStopReason,
    ) {
        self.result_tracer
            .after_vm_execution(state, bootloader_state, stop_reason.clone());

        if let Some(refund_tracer) = &mut self.refund_tracer {
            refund_tracer.after_vm_execution(state, bootloader_state, stop_reason.clone());
        }
        self.dispatcher
            .after_vm_execution(state, bootloader_state, stop_reason.clone());
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the call stack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::interface::VmExecutionMode;
use zk_evm_1_3_3::witness_trace::DummyTracer;
use zk_evm_1_3_3::zkevm_opcode_defs::{Opcode, RetOpcode};
use zk_evm_1_3_3::{
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::Timestamp;

use crate::vm_virtual_blocks::bootloader_state::utils::apply_l2_block;
use crate::vm_virtual_blocks::bootloader_state::BootloaderState;
use crate::vm_virtual_blocks::constants::BOOTLOADER_HEAP_PAGE;
use crate::vm_virtual_blocks::old_vm::history_recorder::HistoryMode;
use crate::vm_virtual_blocks::old_vm::memory::SimpleMemory;
use crate::vm_virtual_blocks::tracers::traits::{
    ExecutionEndTracer, ExecutionProcessing, VmTracer,
};
use crate::vm_virtual_blocks::tracers::utils::{
    computational_gas_price, gas_spent_on_bytecodes_and_long_messages_this_opcode,
    print_debug_if_needed, VmHook,
};
use crate::vm_virtual_blocks::tracers::{RefundsTracer, ResultTracer};
use crate::vm_virtual_blocks::types::internals::ZkSyncVmState;
use crate::vm_virtual_blocks::VmExecutionStopReason;

/// Default tracer for the VM. It manages the other tracers execution and stop the vm when needed.
pub(crate) struct DefaultExecutionTracer<S, H, T> {
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
    pub(crate) refund_tracer: Option<RefundsTracer>,
    pub(crate) custom_tracer: T,
    ret_from_the_bootloader: Option<RetOpcode>,
    storage: StoragePtr<S>,
    _phantom: PhantomData<H>,
}

impl<S, H: HistoryMode, T> Debug for DefaultExecutionTracer<S, H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExecutionTracer").finish()
    }
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H>> Tracer
    for DefaultExecutionTracer<S, H, T>
{
    const CALL_BEFORE_DECODING: bool = false;
    const CALL_AFTER_DECODING: bool = true;
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory<H>;

    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}

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
        if let Some(refund) = &mut self.refund_tracer {
            <RefundsTracer as DynTracer<S, SimpleMemory<H>>>::after_decoding(
                refund, state, data, memory,
            );
        }

        self.custom_tracer.after_decoding(state, data, memory);
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
        if let Some(refund) = &mut self.refund_tracer {
            refund.before_execution(state, data, memory, self.storage.clone());
        }

        self.custom_tracer
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
        if let Some(refund) = &mut self.refund_tracer {
            refund.after_execution(state, data, memory, self.storage.clone());
        }

        self.custom_tracer
            .after_execution(state, data, memory, self.storage.clone());
    }
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H>> ExecutionEndTracer<H>
    for DefaultExecutionTracer<S, H, T>
{
    fn should_stop_execution(&self) -> bool {
        let mut should_stop = match self.execution_mode {
            VmExecutionMode::OneTx => self.tx_has_been_processed(),
            VmExecutionMode::Batch => false,
            VmExecutionMode::Bootloader => self.ret_from_the_bootloader == Some(RetOpcode::Ok),
        };
        should_stop = should_stop || self.validation_run_out_of_gas();
        should_stop = should_stop || self.custom_tracer.should_stop_execution();
        should_stop
    }
}

impl<S: WriteStorage, H: HistoryMode, T> DefaultExecutionTracer<S, H, T> {
    pub(crate) fn new(
        computational_gas_limit: u32,
        execution_mode: VmExecutionMode,
        custom_tracer: T,
        refund_tracer: Option<RefundsTracer>,
        storage: StoragePtr<S>,
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
            custom_tracer,
            ret_from_the_bootloader: None,
            storage,
            _phantom: Default::default(),
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
}

impl<S: WriteStorage, H: HistoryMode, T: VmTracer<S, H>> DefaultExecutionTracer<S, H, T> {
    pub(crate) fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.result_tracer.initialize_tracer(state);
        if let Some(refund) = &mut self.refund_tracer {
            refund.initialize_tracer(state);
        }
        self.custom_tracer.initialize_tracer(state);
    }

    pub(crate) fn before_cycle(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.result_tracer.before_cycle(state);
        if let Some(refund) = &mut self.refund_tracer {
            refund.before_cycle(state);
        }
        self.custom_tracer.before_cycle(state);
    }

    pub(crate) fn after_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) {
        self.result_tracer.after_cycle(state, bootloader_state);
        if let Some(refund) = &mut self.refund_tracer {
            refund.after_cycle(state, bootloader_state);
        }
        self.custom_tracer.after_cycle(state, bootloader_state);
        if self.final_batch_info_requested {
            self.set_fictive_l2_block(state, bootloader_state)
        }
    }

    pub(crate) fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        stop_reason: VmExecutionStopReason,
    ) {
        self.result_tracer
            .after_vm_execution(state, bootloader_state, stop_reason);
        if let Some(refund) = &mut self.refund_tracer {
            refund.after_vm_execution(state, bootloader_state, stop_reason);
        }
        self.custom_tracer
            .after_vm_execution(state, bootloader_state, stop_reason);
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the callstack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

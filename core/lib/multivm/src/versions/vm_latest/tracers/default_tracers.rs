use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

use crate::interface::tracer::{TracerExecutionStopReason, VmExecutionStopReason};
use crate::interface::{Halt, VmExecutionMode};
use zk_evm_1_4_0::{
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{decoding::EncodingModeProduction, Opcode, RetOpcode},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::Timestamp;

use crate::interface::traits::tracers::dyn_tracers::vm_1_4_0::DynTracer;
use crate::interface::types::tracer::TracerExecutionStatus;
use crate::vm_latest::bootloader_state::utils::apply_l2_block;
use crate::vm_latest::bootloader_state::BootloaderState;
use crate::vm_latest::constants::BOOTLOADER_HEAP_PAGE;
use crate::vm_latest::old_vm::history_recorder::HistoryMode;
use crate::vm_latest::old_vm::memory::SimpleMemory;
use crate::vm_latest::tracers::dispatcher::TracerDispatcher;
use crate::vm_latest::tracers::utils::{
    computational_gas_price, gas_spent_on_bytecodes_and_long_messages_this_opcode,
    print_debug_if_needed, VmHook,
};
use crate::vm_latest::tracers::{RefundsTracer, ResultTracer};
use crate::vm_latest::types::internals::ZkSyncVmState;
use crate::vm_latest::VmTracer;

use super::PubdataTracer;

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
    pub(crate) result_tracer: ResultTracer<S>,
    // This tracer is designed specifically for calculating refunds. Its separation from the custom tracer
    // ensures static dispatch, enhancing performance by avoiding dynamic dispatch overhead.
    // Additionally, being an internal tracer, it saves the results directly to VmResultAndLogs.
    pub(crate) refund_tracer: Option<RefundsTracer<S>>,
    // The pubdata tracer is responsible for inserting the pubdata packing information into the bootloader
    // memory at the end of the batch. Its separation from the custom tracer
    // ensures static dispatch, enhancing performance by avoiding dynamic dispatch overhead.
    pub(crate) pubdata_tracer: Option<PubdataTracer<S>>,
    pub(crate) dispatcher: TracerDispatcher<S, H>,
    ret_from_the_bootloader: Option<RetOpcode>,
    storage: StoragePtr<S>,
    _phantom: PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> DefaultExecutionTracer<S, H> {
    pub(crate) fn new(
        computational_gas_limit: u32,
        execution_mode: VmExecutionMode,
        dispatcher: TracerDispatcher<S, H>,
        storage: StoragePtr<S>,
        refund_tracer: Option<RefundsTracer<S>>,
        pubdata_tracer: Option<PubdataTracer<S>>,
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
            pubdata_tracer,
            ret_from_the_bootloader: None,
            storage,
            _phantom: PhantomData,
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

impl<S: WriteStorage, H: HistoryMode> Debug for DefaultExecutionTracer<S, H> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExecutionTracer").finish()
    }
}

/// The default tracer for the VM manages all other tracers. For the sake of optimization, these tracers are statically dispatched.
/// At the same time, the boilerplate for calling these tracers for all tracer calls is quite extensive.
/// This macro is used to reduce the boilerplate.
///
/// Usage:
/// ```
/// dispatch_tracers!(
///   self.after_decoding(state, data, memory)
/// );
/// ```
/// Whenever a new tracer is added, it should be added to the macro call.
///
/// The macro passes the function call to all tracers.
macro_rules! dispatch_tracers {
    ($self:ident.$function:ident($( $params:expr ),*)) => {
       $self.result_tracer.$function($( $params ),*);
       $self.dispatcher.$function($( $params ),*);
        if let Some(tracer) = &mut $self.refund_tracer {
            tracer.$function($( $params ),*);
        }
        if let Some(tracer) = &mut $self.pubdata_tracer {
            tracer.$function($( $params ),*);
        }
    };
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
        dispatch_tracers!(self.after_decoding(state, data, memory));
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

        dispatch_tracers!(self.before_execution(state, data, memory, self.storage.clone()));
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        if let VmExecutionMode::Bootloader = self.execution_mode {
            let (next_opcode, _, _) = zk_evm_1_4_0::vm_state::read_and_decode(
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

        dispatch_tracers!(self.after_execution(state, data, memory, self.storage.clone()));
    }
}

impl<S: WriteStorage, H: HistoryMode> DefaultExecutionTracer<S, H> {
    pub(crate) fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        dispatch_tracers!(self.initialize_tracer(state));
    }

    pub(crate) fn finish_cycle(
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
        if let Some(pubdata_tracer) = &mut self.pubdata_tracer {
            result = pubdata_tracer
                .finish_cycle(state, bootloader_state)
                .stricter(&result);
        }
        result.stricter(&self.should_stop_execution())
    }

    pub(crate) fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        stop_reason: VmExecutionStopReason,
    ) {
        dispatch_tracers!(self.after_vm_execution(state, bootloader_state, stop_reason.clone()));
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the callstack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

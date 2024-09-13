use zk_evm_1_3_1::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
    zkevm_opcode_defs::FatPointer,
};
use zksync_types::U256;

use crate::{
    interface::Call,
    vm_m6::{
        history_recorder::HistoryMode,
        memory::SimpleMemory,
        oracles::tracer::{
            utils::{
                gas_spent_on_bytecodes_and_long_messages_this_opcode, print_debug_if_needed,
                read_pointer, VmHook,
            },
            CallTracer, ExecutionEndTracer, PendingRefundTracer, PubdataSpentTracer,
            StorageInvocationTracer,
        },
        vm_instance::get_vm_hook_params,
    },
};

#[derive(Debug)]
pub(crate) struct TransactionResultTracer<H: HistoryMode> {
    pub(crate) revert_reason: Option<Vec<u8>>,
    gas_spent_on_bytecodes_and_long_messages: u32,
    pub(crate) call_tracer: Option<CallTracer<H>>,
    missed_storage_invocation_limit: usize,
    missed_storage_invocation: usize,
}

impl<H: HistoryMode> TransactionResultTracer<H> {
    pub(crate) fn new(missed_storage_invocation_limit: usize, with_call_tracer: bool) -> Self {
        let call_tracer = if with_call_tracer {
            Some(CallTracer::new())
        } else {
            None
        };
        Self {
            missed_storage_invocation_limit,
            revert_reason: None,
            gas_spent_on_bytecodes_and_long_messages: 0,
            missed_storage_invocation: 0,
            call_tracer,
        }
    }
    pub fn call_trace(&mut self) -> Option<Vec<Call>> {
        self.call_tracer
            .as_mut()
            .map(|call_tracer| call_tracer.extract_calls())
    }
}

impl<H: HistoryMode> Tracer for TransactionResultTracer<H> {
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory<H>;

    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &Self::SupportedMemory,
    ) {
    }
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        if matches!(hook, VmHook::ExecutionResult) {
            let vm_hook_params = get_vm_hook_params(memory);

            let success = vm_hook_params[0];
            let returndata_ptr = FatPointer::from_u256(vm_hook_params[1]);
            let returndata = read_pointer(memory, returndata_ptr);

            if success == U256::zero() {
                self.revert_reason = Some(returndata);
            } else {
                self.revert_reason = None;
            }
        }

        self.gas_spent_on_bytecodes_and_long_messages +=
            gas_spent_on_bytecodes_and_long_messages_this_opcode(&state, &data);
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        if let Some(call_tracer) = self.call_tracer.as_mut() {
            call_tracer.after_execution(state, data, memory);
        }
    }
}

impl<H: HistoryMode> ExecutionEndTracer<H> for TransactionResultTracer<H> {
    // If we reach the limit of memory invocations, we stop the execution and return the error to user
    fn should_stop_execution(&self) -> bool {
        self.is_limit_reached()
    }
}

impl<H: HistoryMode> PendingRefundTracer<H> for TransactionResultTracer<H> {}

impl<H: HistoryMode> PubdataSpentTracer<H> for TransactionResultTracer<H> {
    fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }
}

impl<H: HistoryMode> StorageInvocationTracer<H> for TransactionResultTracer<H> {
    fn set_missed_storage_invocations(&mut self, missed_storage_invocation: usize) {
        self.missed_storage_invocation = missed_storage_invocation;
    }
    fn is_limit_reached(&self) -> bool {
        self.missed_storage_invocation > self.missed_storage_invocation_limit
    }
}

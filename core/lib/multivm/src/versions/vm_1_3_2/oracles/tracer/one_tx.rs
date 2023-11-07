use super::utils::{computational_gas_price, print_debug_if_needed};
use crate::vm_1_3_2::{
    history_recorder::HistoryMode,
    memory::SimpleMemory,
    oracles::tracer::{
        utils::{gas_spent_on_bytecodes_and_long_messages_this_opcode, VmHook},
        BootloaderTracer, ExecutionEndTracer, PendingRefundTracer, PubdataSpentTracer,
    },
    vm_instance::get_vm_hook_params,
};

use crate::vm_1_3_2::oracles::tracer::{CallTracer, StorageInvocationTracer};
use zk_evm_1_3_3::{
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
};
use zksync_types::vm_trace::Call;

/// Allows any opcodes, but tells the VM to end the execution once the tx is over.
// Internally depeds on Bootloader's VMHooks to get the notification once the transaction is finished.
#[derive(Debug)]
pub struct OneTxTracer<H: HistoryMode> {
    tx_has_been_processed: bool,

    // Some(x) means that the bootloader has asked the operator
    // to provide the refund the user, where `x` is the refund proposed
    // by the bootloader itself.
    pending_operator_refund: Option<u32>,

    pub refund_gas: u32,
    pub gas_spent_on_bytecodes_and_long_messages: u32,

    // Amount of gas used during account validation.
    computational_gas_used: u32,
    // Maximum number of gas that we're allowed to use during account validation.
    computational_gas_limit: u32,
    in_account_validation: bool,

    bootloader_tracer: BootloaderTracer<H>,
    call_tracer: Option<CallTracer<H>>,
}

impl<H: HistoryMode> Tracer for OneTxTracer<H> {
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
        if self.in_account_validation {
            self.computational_gas_used = self
                .computational_gas_used
                .saturating_add(computational_gas_price(state, &data));
        }

        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        match hook {
            VmHook::TxHasEnded => self.tx_has_been_processed = true,
            VmHook::NotifyAboutRefund => self.refund_gas = get_vm_hook_params(memory)[0].as_u32(),
            VmHook::AskOperatorForRefund => {
                self.pending_operator_refund = Some(get_vm_hook_params(memory)[0].as_u32())
            }
            VmHook::NoValidationEntered => self.in_account_validation = false,
            VmHook::AccountValidationEntered => self.in_account_validation = true,
            _ => {}
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
        self.bootloader_tracer.after_execution(state, data, memory);
        if let Some(call_tracer) = self.call_tracer.as_mut() {
            call_tracer.after_execution(state, data, memory);
        }
    }
}

impl<H: HistoryMode> ExecutionEndTracer<H> for OneTxTracer<H> {
    fn should_stop_execution(&self) -> bool {
        self.tx_has_been_processed
            || self.bootloader_tracer.should_stop_execution()
            || self.validation_run_out_of_gas()
    }
}

impl<H: HistoryMode> PendingRefundTracer<H> for OneTxTracer<H> {
    fn requested_refund(&self) -> Option<u32> {
        self.pending_operator_refund
    }

    fn set_refund_as_done(&mut self) {
        self.pending_operator_refund = None;
    }
}

impl<H: HistoryMode> PubdataSpentTracer<H> for OneTxTracer<H> {
    fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }
}

impl<H: HistoryMode> StorageInvocationTracer<H> for OneTxTracer<H> {}

impl<H: HistoryMode> OneTxTracer<H> {
    pub fn new(computational_gas_limit: u32, with_call_tracer: bool) -> Self {
        let call_tracer = if with_call_tracer {
            Some(CallTracer::new())
        } else {
            None
        };
        Self {
            tx_has_been_processed: false,
            pending_operator_refund: None,
            refund_gas: 0,
            gas_spent_on_bytecodes_and_long_messages: 0,
            computational_gas_used: 0,
            computational_gas_limit,
            in_account_validation: false,
            bootloader_tracer: BootloaderTracer::default(),
            call_tracer,
        }
    }

    pub fn is_bootloader_out_of_gas(&self) -> bool {
        self.bootloader_tracer.is_bootloader_out_of_gas()
    }

    pub fn tx_has_been_processed(&self) -> bool {
        self.tx_has_been_processed
    }

    pub fn validation_run_out_of_gas(&self) -> bool {
        self.computational_gas_used > self.computational_gas_limit
    }

    pub fn call_traces(&mut self) -> Vec<Call> {
        self.call_tracer
            .as_mut()
            .map_or(vec![], |call_tracer| call_tracer.extract_calls())
    }
}

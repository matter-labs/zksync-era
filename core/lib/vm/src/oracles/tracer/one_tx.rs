use super::utils::{computational_gas_price, print_debug_if_needed};
use crate::{
    memory::SimpleMemory,
    oracles::tracer::{
        utils::VmHook, BootloaderTracer, ExecutionEndTracer, PendingRefundTracer,
        PubdataSpentTracer,
    },
    vm::get_vm_hook_params,
};

use zk_evm::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
    zkevm_opcode_defs::{LogOpcode, Opcode},
};
use zksync_config::constants::{KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS};

/// Allows any opcodes, but tells the VM to end the execution once the tx is over.
#[derive(Debug, Clone)]
pub struct OneTxTracer {
    tx_has_been_processed: bool,

    // Some(x) means that the bootloader has asked the operator
    // to provide the refund the user, where `x` is the refund proposed
    // by the bootloader itself.
    pending_operator_refund: Option<u32>,

    pub refund_gas: u32,
    pub gas_spent_on_bytecodes_and_long_messages: u32,

    computational_gas_used: u32,
    computational_gas_limit: u32,
    in_account_validation: bool,

    bootloader_tracer: BootloaderTracer,
}

impl Tracer for OneTxTracer {
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory;

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

        if data.opcode.variant.opcode == Opcode::Log(LogOpcode::PrecompileCall) {
            let current_stack = state.vm_local_state.callstack.get_current_stack();
            // Trace for precompile calls from `KNOWN_CODES_STORAGE_ADDRESS` and `L1_MESSENGER_ADDRESS` that burn some gas.
            // Note, that if there is less gas left than requested to burn it will be burnt anyway.
            if current_stack.this_address == KNOWN_CODES_STORAGE_ADDRESS
                || current_stack.this_address == L1_MESSENGER_ADDRESS
            {
                self.gas_spent_on_bytecodes_and_long_messages +=
                    std::cmp::min(data.src1_value.value.as_u32(), current_stack.ergs_remaining);
            }
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        self.bootloader_tracer.after_execution(state, data, memory)
    }
}

impl ExecutionEndTracer for OneTxTracer {
    fn should_stop_execution(&self) -> bool {
        self.tx_has_been_processed
            || self.bootloader_tracer.should_stop_execution()
            || self.validation_run_out_of_gas()
    }
}

impl PendingRefundTracer for OneTxTracer {
    fn requested_refund(&self) -> Option<u32> {
        self.pending_operator_refund
    }

    fn set_refund_as_done(&mut self) {
        self.pending_operator_refund = None;
    }
}

impl PubdataSpentTracer for OneTxTracer {
    fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
    }
}

impl OneTxTracer {
    pub fn new(computational_gas_limit: u32) -> Self {
        Self {
            tx_has_been_processed: false,
            pending_operator_refund: None,
            refund_gas: 0,
            gas_spent_on_bytecodes_and_long_messages: 0,
            computational_gas_used: 0,
            computational_gas_limit,
            in_account_validation: false,
            bootloader_tracer: BootloaderTracer::default(),
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
}

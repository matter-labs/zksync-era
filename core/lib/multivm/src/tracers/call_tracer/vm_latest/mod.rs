use std::str::FromStr;

use zk_evm_1_5_0::{
    tracing::{AfterExecutionData, BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{
        FarCallABI, FatPointer, Opcode, RetOpcode, UMAOpcode,
        CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
    },
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    vm_trace::{Call, CallType},
    zk_evm_types::FarCallOpcode,
    U256,
};

use crate::{
    glue::GlueInto,
    interface::{
        tracer::VmExecutionStopReason, traits::tracers::dyn_tracers::vm_1_5_0::DynTracer,
        VmRevertReason,
    },
    tracers::call_tracer::CallTracer,
    vm_latest::{
        utils::heap_page_from_base, BootloaderState, HistoryMode, SimpleMemory, VmTracer,
        ZkSyncVmState,
    },
};

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CallTracer {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        // FIXME: this catches not only Evm contracts

        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        const DEBUG_SLOT: u32 = 32 * 32;

        let debug_magic = U256::from_dec_str(
            "33509158800074003487174289148292687789659295220513886355337449724907776218753",
        )
        .unwrap();

        // Only `UMA` opcodes in the bootloader serve for vm hooks
        if !matches!(opcode_variant.opcode, Opcode::UMA(UMAOpcode::HeapWrite))
            || fat_ptr.offset != DEBUG_SLOT
            || value != debug_magic
        {
            // println!("I tried");
            return;
        }

        let how_to_print_value = memory.read_slot(heap_page as usize, 32 + 1).value;
        let value_to_print = memory.read_slot(heap_page as usize, 32 + 2).value;

        let print_as_hex_value =
            U256::from_str("0x00debdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebde")
                .unwrap();
        let print_as_string_value =
            U256::from_str("0x00debdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdf")
                .unwrap();

        if how_to_print_value == print_as_hex_value {
            print!("PRINTED: ");
            println!("0x{:02x}", value_to_print);
        }

        if how_to_print_value == print_as_string_value {
            print!("PRINTED: ");
            let mut value = value_to_print.0;
            value.reverse();
            for limb in value {
                print!(
                    "{}",
                    String::from_utf8(limb.to_be_bytes().to_vec()).unwrap()
                );
            }
            println!("");
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        match data.opcode.variant.opcode {
            Opcode::NearCall(_) => {
                self.increase_near_call_count();
            }
            Opcode::FarCall(far_call) => {
                // We use parent gas for properly calculating gas used in the trace.
                let current_ergs = state.vm_local_state.callstack.current.ergs_remaining;
                let parent_gas = state
                    .vm_local_state
                    .callstack
                    .inner
                    .last()
                    .map(|call| call.ergs_remaining + current_ergs)
                    .unwrap_or(current_ergs);

                let mut current_call = Call {
                    r#type: CallType::Call(far_call.glue_into()),
                    gas: 0,
                    parent_gas,
                    ..Default::default()
                };

                self.handle_far_call_op_code_latest(state, memory, &mut current_call);
                self.push_call_and_update_stats(current_call, 0);
            }
            Opcode::Ret(ret_code) => {
                self.handle_ret_op_code_latest(state, memory, ret_code);
            }
            _ => {}
        };
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CallTracer {
    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
        self.store_result()
    }
}

impl CallTracer {
    fn handle_far_call_op_code_latest<H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
        current_call: &mut Call,
    ) {
        let current = state.vm_local_state.callstack.current;
        // All calls from the actual users are mimic calls,
        // so we need to check that the previous call was to the deployer.
        // Actually it's a call of the constructor.
        // And at this stage caller is user and callee is deployed contract.
        let call_type = if let CallType::Call(far_call) = current_call.r#type {
            if matches!(far_call, FarCallOpcode::Mimic) {
                let previous_caller = state
                    .vm_local_state
                    .callstack
                    .inner
                    .last()
                    .map(|call| call.this_address)
                    // Actually it's safe to just unwrap here, because we have at least one call in the stack
                    // But i want to be sure that we will not have any problems in the future
                    .unwrap_or(current.this_address);
                if previous_caller == CONTRACT_DEPLOYER_ADDRESS {
                    CallType::Create
                } else {
                    CallType::Call(far_call)
                }
            } else {
                CallType::Call(far_call)
            }
        } else {
            unreachable!()
        };
        let calldata = if current.code_page.0 == 0 || current.ergs_remaining == 0 {
            vec![]
        } else {
            let packed_abi =
                state.vm_local_state.registers[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];
            assert!(packed_abi.is_pointer);
            let far_call_abi = FarCallABI::from_u256(packed_abi.value);
            memory.read_unaligned_bytes(
                far_call_abi.memory_quasi_fat_pointer.memory_page as usize,
                far_call_abi.memory_quasi_fat_pointer.start as usize,
                far_call_abi.memory_quasi_fat_pointer.length as usize,
            )
        };

        current_call.input = calldata;
        current_call.r#type = call_type;
        current_call.from = current.msg_sender;
        current_call.to = current.this_address;
        current_call.value = U256::from(current.context_u128_value);
        current_call.gas = current.ergs_remaining;
    }

    fn save_output_latest<H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
        ret_opcode: RetOpcode,
        current_call: &mut Call,
    ) {
        let fat_data_pointer =
            state.vm_local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];

        // if `fat_data_pointer` is not a pointer then there is no output
        let output = if fat_data_pointer.is_pointer {
            let fat_data_pointer = FatPointer::from_u256(fat_data_pointer.value);
            if !fat_data_pointer.is_trivial() {
                Some(memory.read_unaligned_bytes(
                    fat_data_pointer.memory_page as usize,
                    fat_data_pointer.start as usize,
                    fat_data_pointer.length as usize,
                ))
            } else {
                None
            }
        } else {
            None
        };

        match ret_opcode {
            RetOpcode::Ok => {
                current_call.output = output.unwrap_or_default();
            }
            RetOpcode::Revert => {
                if let Some(output) = output {
                    current_call.revert_reason =
                        Some(VmRevertReason::from(output.as_slice()).to_string());
                } else {
                    current_call.revert_reason = Some("Unknown revert reason".to_string());
                }
            }
            RetOpcode::Panic => {
                current_call.error = Some("Panic".to_string());
            }
        }
    }

    fn handle_ret_op_code_latest<H: HistoryMode>(
        &mut self,
        state: VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
        ret_opcode: RetOpcode,
    ) {
        let Some(mut current_call) = self.stack.pop() else {
            return;
        };

        if current_call.near_calls_after > 0 {
            current_call.near_calls_after -= 1;
            self.push_call_and_update_stats(current_call.farcall, current_call.near_calls_after);
            return;
        }

        current_call.farcall.gas_used = current_call
            .farcall
            .parent_gas
            .saturating_sub(state.vm_local_state.callstack.current.ergs_remaining);

        self.save_output_latest(state, memory, ret_opcode, &mut current_call.farcall);

        // If there is a parent call, push the current call to it
        // Otherwise, push the current call to the stack, because it's the top level call
        if let Some(parent_call) = self.stack.last_mut() {
            parent_call.farcall.calls.push(current_call.farcall);
        } else {
            self.push_call_and_update_stats(current_call.farcall, current_call.near_calls_after);
        }
    }
}

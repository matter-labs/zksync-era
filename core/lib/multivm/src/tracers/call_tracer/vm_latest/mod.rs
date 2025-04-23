use zk_evm_1_5_2::{
    tracing::{AfterExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{
        FarCallABI, FatPointer, Opcode, RetOpcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
        RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
    },
};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{zk_evm_types::FarCallOpcode, U256};

use crate::{
    glue::GlueInto,
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::VmExecutionStopReason,
        Call, CallType, VmRevertReason,
    },
    tracers::{dynamic::vm_1_5_2::DynTracer, CallTracer},
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CallTracer {
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
                    .unwrap_or(current_ergs) as u64;

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
        let result = std::mem::take(&mut self.finished_calls);
        let cell = self.result.as_ref();
        cell.set(result).unwrap();
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
        current_call.gas = current.ergs_remaining as u64;
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
            .saturating_sub(state.vm_local_state.callstack.current.ergs_remaining as u64);
        self.save_output_latest(state, memory, ret_opcode, &mut current_call.farcall);

        // If there is a parent call, push the current call to it
        // Otherwise, push the current call to the stack, because it's the top level call
        if let Some(parent_call) = self.stack.last_mut() {
            parent_call.farcall.calls.push(current_call.farcall);
        } else {
            self.finished_calls.push(current_call.farcall);
        }
    }
}

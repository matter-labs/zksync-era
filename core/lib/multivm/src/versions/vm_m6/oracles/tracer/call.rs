use std::{convert::TryFrom, marker::PhantomData, mem};

use zk_evm_1_3_1::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    zkevm_opcode_defs::{
        FarCallABI, FarCallOpcode, FatPointer, Opcode, RetOpcode,
        CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
    },
};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::U256;

use crate::{
    glue::GlueInto,
    interface::{Call, CallType},
    vm_m6::{errors::VmRevertReason, history_recorder::HistoryMode, memory::SimpleMemory},
};

/// NOTE Auto implementing clone for this tracer can cause stack overflow.
///
/// This is because of the stack field which is a Vec with nested vecs inside.
/// If you will need to implement clone for this tracer, please consider to not copy the stack field.
/// Method `extract_calls` will extract the necessary stack for you.
#[derive(Debug, Default)]
pub struct CallTracer<H: HistoryMode> {
    stack: Vec<Call>,
    _phantom: PhantomData<H>,
}

impl<H: HistoryMode> CallTracer<H> {
    pub fn new() -> Self {
        Self {
            stack: vec![],
            _phantom: PhantomData,
        }
    }
}

impl<H: HistoryMode> Tracer for CallTracer<H> {
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
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &Self::SupportedMemory,
    ) {
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        let call_type = match data.opcode.variant.opcode {
            Opcode::NearCall(_) => CallType::NearCall,
            Opcode::FarCall(far_call) => CallType::Call(far_call.glue_into()),
            Opcode::Ret(ret_code) => {
                self.handle_ret_op_code(state, data, memory, ret_code);
                return;
            }
            _ => {
                return;
            }
        };

        let mut current_call = Call {
            r#type: call_type,
            gas: 0,
            ..Default::default()
        };
        match call_type {
            CallType::Call(_) | CallType::Create => {
                self.handle_far_call_op_code(state, data, memory, &mut current_call)
            }
            CallType::NearCall => {
                self.handle_near_call_op_code(state, data, memory, &mut current_call);
            }
        }
        self.stack.push(current_call);
    }
}

impl<H: HistoryMode> CallTracer<H> {
    /// We use parent gas for property calculation of gas used in the trace.
    /// This method updates parent gas for the current call.
    fn update_parent_gas(&mut self, state: &VmLocalStateData<'_>, current_call: &mut Call) {
        let current = state.vm_local_state.callstack.current;
        let parent_gas = state
            .vm_local_state
            .callstack
            .inner
            .last()
            .map(|call| call.ergs_remaining + current.ergs_remaining)
            .unwrap_or(current.ergs_remaining) as u64;
        current_call.parent_gas = parent_gas;
    }

    fn handle_near_call_op_code(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &SimpleMemory<H>,
        current_call: &mut Call,
    ) {
        self.update_parent_gas(&state, current_call);
    }

    fn handle_far_call_op_code(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        current_call: &mut Call,
    ) {
        self.update_parent_gas(&state, current_call);
        let current = state.vm_local_state.callstack.current;
        // All calls from the actual users are mimic calls,
        // so we need to check that the previous call was to the deployer.
        // Actually it's a call of the constructor.
        // And at this stage caller is user and callee is deployed contract.
        let call_type = if let CallType::Call(far_call) = current_call.r#type {
            if matches!(far_call.glue_into(), FarCallOpcode::Mimic) {
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

    fn save_output(
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
                    match VmRevertReason::try_from(output.as_slice()) {
                        Ok(rev) => {
                            current_call.revert_reason = Some(rev.to_string());
                        }
                        Err(_) => {
                            current_call.revert_reason = Some(format!("{:?}", hex::encode(output)));
                        }
                    }
                } else {
                    current_call.revert_reason = Some("Unknown revert reason".to_string());
                }
            }
            RetOpcode::Panic => {
                current_call.error = Some("Panic".to_string());
            }
        }
    }

    fn handle_ret_op_code(
        &mut self,
        state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        ret_opcode: RetOpcode,
    ) {
        // It's safe to unwrap here because we are sure that we have at least one call in the stack
        let mut current_call = self.stack.pop().unwrap();
        current_call.gas_used =
            current_call.parent_gas - state.vm_local_state.callstack.current.ergs_remaining as u64;

        if current_call.r#type != CallType::NearCall {
            self.save_output(state, memory, ret_opcode, &mut current_call);
        }

        // If there is a parent call, push the current call to it
        // Otherwise, push the current call to the stack, because it's the top level call
        if let Some(parent_call) = self.stack.last_mut() {
            parent_call.calls.push(current_call);
        } else {
            self.stack.push(current_call);
        }
    }

    // Filter all near calls from the call stack
    // Important that the very first call is near call
    // And this `NearCall` includes several Normal or Mimic calls
    // So we return all children of this `NearCall`
    pub fn extract_calls(&mut self) -> Vec<Call> {
        if let Some(current_call) = self.stack.pop() {
            filter_near_call(current_call)
        } else {
            vec![]
        }
    }
}

// Filter all near calls from the call stack
// Normally we are not interested in `NearCall`, because it's just a wrapper for internal calls
fn filter_near_call(mut call: Call) -> Vec<Call> {
    let mut calls = vec![];
    let original_calls = std::mem::take(&mut call.calls);
    for call in original_calls {
        calls.append(&mut filter_near_call(call));
    }
    call.calls = calls;

    if call.r#type == CallType::NearCall {
        mem::take(&mut call.calls)
    } else {
        vec![call]
    }
}

#[cfg(test)]
mod tests {
    use zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode;

    use crate::{
        glue::GlueInto,
        vm_m6::oracles::tracer::call::{filter_near_call, Call, CallType},
    };

    #[test]
    fn test_filter_near_calls() {
        let mut call = Call::default();
        let filtered_call = filter_near_call(call.clone());
        assert_eq!(filtered_call.len(), 1);

        let mut near_call = call.clone();
        near_call.r#type = CallType::NearCall;
        let filtered_call = filter_near_call(near_call.clone());
        assert_eq!(filtered_call.len(), 0);

        call.r#type = CallType::Call(FarCallOpcode::Mimic.glue_into());
        call.calls = vec![Call::default(), Call::default(), near_call.clone()];
        let filtered_call = filter_near_call(call.clone());
        assert_eq!(filtered_call.len(), 1);
        assert_eq!(filtered_call[0].calls.len(), 2);

        near_call.calls = vec![Call::default(), Call::default(), near_call.clone()];
        call.calls = vec![Call::default(), Call::default(), near_call];
        let filtered_call = filter_near_call(call);
        assert_eq!(filtered_call.len(), 1);
        assert_eq!(filtered_call[0].calls.len(), 4);
    }
}

use once_cell::sync::OnceCell;
use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;

use zk_evm::tracing::{AfterExecutionData, VmLocalStateData};
use zk_evm::zkevm_opcode_defs::{
    FarCallABI, FarCallOpcode, FatPointer, Opcode, RetOpcode,
    CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
};

use zksync_config::constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::vm_trace::{Call, CallType};
use zksync_types::U256;

use crate::errors::VmRevertReason;
use crate::old_vm::history_recorder::HistoryMode;
use crate::old_vm::memory::SimpleMemory;
use crate::tracers::traits::{DynTracer, ExecutionEndTracer, ExecutionProcessing, VmTracer};
use crate::types::outputs::VmExecutionResultAndLogs;

/// Maximum depth of the call stack.
/// This is used to prevent stack overflow.
const MAX_DEPTH: u32 = 1000;

/// NOTE Auto implementing clone for this tracer can cause stack overflow.
/// This is because of the stack field which is a Vec with nested vecs inside.
/// If you will need to implement clone for this tracer, please consider to not copy the stack field.
#[derive(Debug)]
pub struct CallTracer<H: HistoryMode> {
    stack: Vec<Call>,
    result: Arc<OnceCell<Vec<Call>>>,
    _phantom: PhantomData<fn() -> H>,
}

impl<H: HistoryMode> CallTracer<H> {
    pub fn new(resulted_stack: Arc<OnceCell<Vec<Call>>>, _history: H) -> Self {
        Self {
            stack: vec![],
            result: resulted_stack,
            _phantom: PhantomData,
        }
    }
}

impl<S, H: HistoryMode> DynTracer<S, H> for CallTracer<H> {
    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        let call_type = match data.opcode.variant.opcode {
            Opcode::NearCall(_) => CallType::NearCall,
            Opcode::FarCall(far_call) => CallType::Call(far_call),
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

impl<H: HistoryMode> ExecutionEndTracer<H> for CallTracer<H> {}

impl<S: WriteStorage, H: HistoryMode> ExecutionProcessing<S, H> for CallTracer<H> {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CallTracer<H> {
    fn save_results(&mut self, _result: &mut VmExecutionResultAndLogs) {
        let calls = self.extract_calls();
        self.result.set(calls).expect("Result is already set");
    }
}

impl<H: HistoryMode> CallTracer<H> {
    /// We use parent gas for propery calculation of gas used in the trace.
    /// This method updates parent gas for the current call.
    fn update_parent_gas(&mut self, state: &VmLocalStateData<'_>, current_call: &mut Call) {
        let current = state.vm_local_state.callstack.current;
        let parent_gas = state
            .vm_local_state
            .callstack
            .inner
            .last()
            .map(|call| call.ergs_remaining + current.ergs_remaining)
            .unwrap_or(current.ergs_remaining);
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

    fn save_output(
        &mut self,
        state: VmLocalStateData<'_>,
        memory: &SimpleMemory<H>,
        ret_opcode: RetOpcode,
        current_call: &mut Call,
    ) {
        let fat_data_pointer =
            state.vm_local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];

        // if fat_data_pointer is not a pointer then there is no output
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
            current_call.parent_gas - state.vm_local_state.callstack.current.ergs_remaining;

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
    // And this NearCall includes several Normal or Mimic calls
    // So we return all childrens of this NearCall
    fn extract_calls(&mut self) -> Vec<Call> {
        if let Some(current_call) = self.stack.pop() {
            filter_near_call(current_call, 0)
        } else {
            vec![]
        }
    }
}

// Filter all near calls from the call stack
// Normally wr are not interested in NearCall, because it's just a wrapper for internal calls
fn filter_near_call(mut call: Call, depth: u32) -> Vec<Call> {
    if depth > MAX_DEPTH {
        return vec![];
    }

    let mut calls = vec![];
    let original_calls = std::mem::take(&mut call.calls);
    for call in original_calls {
        calls.append(&mut filter_near_call(call, depth + 1));
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
    use crate::tracers::call::MAX_DEPTH;
    use zk_evm::zkevm_opcode_defs::FarCallOpcode;

    use super::{filter_near_call, Call, CallType};

    #[test]
    fn test_filter_near_calls() {
        let mut call = Call::default();
        let filtered_call = filter_near_call(call.clone(), 0);
        assert_eq!(filtered_call.len(), 1);

        let mut near_call = call.clone();
        near_call.r#type = CallType::NearCall;
        let filtered_call = filter_near_call(near_call.clone(), 0);
        assert_eq!(filtered_call.len(), 0);

        call.r#type = CallType::Call(FarCallOpcode::Mimic);
        call.calls = vec![Call::default(), Call::default(), near_call.clone()];
        let filtered_call = filter_near_call(call.clone(), 0);
        assert_eq!(filtered_call.len(), 1);
        assert_eq!(filtered_call[0].calls.len(), 2);

        let mut near_call = near_call;
        near_call.calls = vec![Call::default(), Call::default(), near_call.clone()];
        call.calls = vec![Call::default(), Call::default(), near_call];
        let filtered_call = filter_near_call(call, 0);
        assert_eq!(filtered_call.len(), 1);
        assert_eq!(filtered_call[0].calls.len(), 4);
    }

    #[test]
    fn test_maximum_depth() {
        let mut call = Call::default();
        let near_call = Call::default();
        call.calls = vec![near_call];
        for _ in 0..MAX_DEPTH {
            let near_call = Call {
                r#type: CallType::NearCall,
                calls: vec![call],
                ..Default::default()
            };
            call = near_call;
        }

        let filtered_call = filter_near_call(call, 0);
        assert_eq!(filtered_call.len(), 1);
    }
}

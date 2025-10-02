use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{zk_evm_types::FarCallOpcode, U256};
use zksync_vm2::{
    interface::{
        CallframeInterface, CallingMode, Opcode, OpcodeType, ReturnType, ShouldStop,
        StateInterface, Tracer,
    },
    FatPointer,
};

use crate::{
    interface::{Call, CallType, VmRevertReason},
    vm_fast::utils::read_raw_fat_pointer,
};

/// Call tracer for the fast VM.
#[derive(Debug, Clone, Default)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
    finished_calls: Vec<Call>,
    current_stack_depth: usize,
    // TODO: report as metrics
    max_stack_depth: usize,
    max_near_calls: usize,
}

#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
}

impl CallTracer {
    /// Converts this tracer into the captured calls.
    pub fn into_result(self) -> Vec<Call> {
        self.finished_calls
    }
}

impl Tracer for CallTracer {
    fn after_instruction<OP: OpcodeType, S: StateInterface>(
        &mut self,
        state: &mut S,
    ) -> ShouldStop {
        match OP::VALUE {
            Opcode::FarCall(ty) => {
                self.current_stack_depth += 1;
                self.max_stack_depth = self.max_stack_depth.max(self.current_stack_depth);

                let current_gas = state.current_frame().gas() as u64;
                let from = state.current_frame().caller();
                let to = state.current_frame().address();
                let input = if current_gas == 0 {
                    vec![]
                } else {
                    read_raw_fat_pointer(state, state.read_register(1).0)
                };
                let value = U256::from(state.current_frame().context_u128());
                let ty = match ty {
                    CallingMode::Normal => CallType::Call(FarCallOpcode::Normal),
                    CallingMode::Delegate => CallType::Call(FarCallOpcode::Delegate),
                    CallingMode::Mimic => {
                        let prev_this_address = state.callframe(1).address();
                        if prev_this_address == CONTRACT_DEPLOYER_ADDRESS {
                            // EraVM contract creation is encoded as a mimic call from `ContractDeployer` to the created contract.
                            CallType::Create
                        } else {
                            CallType::Call(FarCallOpcode::Mimic)
                        }
                    }
                };

                self.stack.push(FarcallAndNearCallCount {
                    farcall: Call {
                        r#type: ty,
                        from,
                        to,
                        // The previous frame always exists directly after a far call
                        parent_gas: current_gas + state.callframe(1).gas() as u64,
                        gas: current_gas,
                        input,
                        value,
                        ..Default::default()
                    },
                    near_calls_after: 0,
                });
            }
            Opcode::NearCall => {
                self.current_stack_depth += 1;
                self.max_stack_depth = self.max_stack_depth.max(self.current_stack_depth);

                if let Some(frame) = self.stack.last_mut() {
                    frame.near_calls_after += 1;
                    self.max_near_calls = self.max_near_calls.max(frame.near_calls_after);
                }
            }
            Opcode::Ret(variant) => {
                self.current_stack_depth -= 1;

                let Some(mut current_call) = self.stack.pop() else {
                    return ShouldStop::Continue;
                };

                if current_call.near_calls_after == 0 {
                    // Might overflow due to stipend
                    current_call.farcall.gas_used = current_call
                        .farcall
                        .parent_gas
                        .saturating_sub(state.current_frame().gas() as u64);

                    let (maybe_output_ptr, is_pointer) = state.read_register(1);
                    let output = if is_pointer {
                        let output_ptr = FatPointer::from(maybe_output_ptr);
                        if output_ptr.length == 0 && output_ptr.offset == 0 {
                            // Trivial pointer, which is formally cannot be dereferenced. This only matters
                            // when extracting the revert reason; the legacy VM treats the trivial pointer specially.
                            None
                        } else {
                            Some(read_raw_fat_pointer(state, maybe_output_ptr))
                        }
                    } else {
                        None
                    };

                    match variant {
                        ReturnType::Normal => {
                            current_call.farcall.output = output.unwrap_or_default();
                        }
                        ReturnType::Revert => {
                            current_call.farcall.revert_reason =
                                Some(if let Some(output) = &output {
                                    VmRevertReason::from(output.as_slice()).to_string()
                                } else {
                                    "Unknown revert reason".to_owned()
                                });
                        }
                        ReturnType::Panic => {
                            current_call.farcall.error = Some("Panic".to_string());
                        }
                    }

                    // If there is a parent call, push the current call to it
                    // Otherwise, put the current call back on the stack, because it's the top level call
                    if let Some(parent_call) = self.stack.last_mut() {
                        parent_call.farcall.calls.push(current_call.farcall);
                    } else {
                        self.finished_calls.push(current_call.farcall);
                    }
                } else {
                    current_call.near_calls_after -= 1;
                    self.stack.push(current_call);
                }
            }
            _ => {}
        }

        ShouldStop::Continue
    }
}

use zksync_types::{zk_evm_types::FarCallOpcode, U256};
use zksync_vm2::{
    interface::{CallframeInterface, Opcode, OpcodeType, ReturnType, StateInterface, Tracer},
    FatPointer,
};
use zksync_vm_interface::{Call, VmRevertReason};

#[derive(Debug, Clone, Default)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
    finished_calls: Vec<Call>,

    current_stack_depth: usize,
    max_stack_depth: usize,

    max_near_calls: usize,
}

#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
}

impl CallTracer {
    pub fn result(self) -> Vec<Call> {
        self.finished_calls
    }
}

impl Tracer for CallTracer {
    fn after_instruction<OP: OpcodeType, S: StateInterface>(&mut self, state: &mut S) {
        match OP::VALUE {
            Opcode::FarCall(tipe) => {
                self.current_stack_depth += 1;
                self.max_stack_depth = self.max_stack_depth.max(self.current_stack_depth);

                let current_gas = state.current_frame().gas() as u64;
                let from = state.current_frame().caller();
                let to = state.current_frame().address();
                let input = read_fat_pointer(state, state.read_register(1).0);
                let value = U256::from(state.current_frame().context_u128());

                self.stack.push(FarcallAndNearCallCount {
                    farcall: Call {
                        r#type: match tipe {
                            zksync_vm2::interface::CallingMode::Normal => {
                                zksync_vm_interface::CallType::Call(FarCallOpcode::Normal)
                            }
                            zksync_vm2::interface::CallingMode::Delegate => {
                                zksync_vm_interface::CallType::Call(FarCallOpcode::Delegate)
                            }
                            zksync_vm2::interface::CallingMode::Mimic => {
                                zksync_vm_interface::CallType::Call(FarCallOpcode::Mimic)
                            }
                        },
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
                    return;
                };

                if current_call.near_calls_after == 0 {
                    // Might overflow due to stipend
                    current_call.farcall.gas_used = current_call
                        .farcall
                        .parent_gas
                        .saturating_sub(state.current_frame().gas() as u64);

                    let output = read_fat_pointer(state, state.read_register(1).0);

                    match variant {
                        ReturnType::Normal => {
                            current_call.farcall.output = output;
                        }
                        ReturnType::Revert => {
                            current_call.farcall.revert_reason =
                                Some(VmRevertReason::from(output.as_slice()).to_string());
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
    }
}

fn read_fat_pointer<S: StateInterface>(state: &S, raw: U256) -> Vec<u8> {
    let pointer = FatPointer::from(raw);
    let length = pointer.length - pointer.offset;
    let start = pointer.start + pointer.offset;
    let mut calldata = vec![0; length as usize];
    for i in 0..length {
        calldata[i as usize] = state.read_heap_byte(pointer.memory_page, start + i);
    }
    calldata
}

use zk_evm_1_3_3::{
    tracing::{AfterDecodingData, BeforeExecutionData, VmLocalStateData},
    vm_state::{ErrorFlags, VmLocalState},
    zkevm_opcode_defs::FatPointer,
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::interface::tracer::{TracerExecutionStopReason, VmExecutionStopReason};
use crate::interface::{ExecutionResult, Halt, TxRevertReason, VmExecutionMode, VmRevertReason};
use zksync_types::U256;

use crate::vm_refunds_enhancement::bootloader_state::BootloaderState;
use crate::vm_refunds_enhancement::old_vm::{
    history_recorder::HistoryMode,
    memory::SimpleMemory,
    utils::{vm_may_have_ended_inner, VmExecutionResult},
};
use crate::vm_refunds_enhancement::tracers::utils::{get_vm_hook_params, read_pointer, VmHook};

use crate::vm_refunds_enhancement::constants::{BOOTLOADER_HEAP_PAGE, RESULT_SUCCESS_FIRST_SLOT};
use crate::vm_refunds_enhancement::types::internals::ZkSyncVmState;
use crate::vm_refunds_enhancement::VmTracer;

#[derive(Debug, Clone)]
enum Result {
    Error { error_reason: VmRevertReason },
    Success { return_data: Vec<u8> },
    Halt { reason: Halt },
}

/// Tracer responsible for handling the VM execution result.
#[derive(Debug, Clone)]
pub(crate) struct ResultTracer {
    result: Option<Result>,
    bootloader_out_of_gas: bool,
    execution_mode: VmExecutionMode,
}

impl ResultTracer {
    pub(crate) fn new(execution_mode: VmExecutionMode) -> Self {
        Self {
            result: None,
            bootloader_out_of_gas: false,
            execution_mode,
        }
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the callstack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for ResultTracer {
    fn after_decoding(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        _memory: &SimpleMemory<H>,
    ) {
        // We should check not only for the `NOT_ENOUGH_ERGS` flag but if the current frame is bootloader too.
        if current_frame_is_bootloader(state.vm_local_state)
            && data
                .error_flags_accumulated
                .contains(ErrorFlags::NOT_ENOUGH_ERGS)
        {
            self.bootloader_out_of_gas = true;
        }
    }

    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        let hook = VmHook::from_opcode_memory(&state, &data);
        if let VmHook::ExecutionResult = hook {
            let vm_hook_params = get_vm_hook_params(memory);
            let success = vm_hook_params[0];
            let returndata_ptr = FatPointer::from_u256(vm_hook_params[1]);
            let returndata = read_pointer(memory, returndata_ptr);
            if success == U256::zero() {
                self.result = Some(Result::Error {
                    // Tx has reverted, without bootloader error, we can simply parse the revert reason
                    error_reason: (VmRevertReason::from(returndata.as_slice())),
                });
            } else {
                self.result = Some(Result::Success {
                    return_data: returndata,
                });
            }
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for ResultTracer {
    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        stop_reason: VmExecutionStopReason,
    ) {
        match stop_reason {
            // Vm has finished execution, we need to check the result of it
            VmExecutionStopReason::VmFinished => {
                self.vm_finished_execution(state);
            }
            // One of the tracers above has requested to stop the execution.
            // If it was the correct stop we already have the result,
            // otherwise it can be out of gas error
            VmExecutionStopReason::TracerRequestedStop(reason) => {
                match self.execution_mode {
                    VmExecutionMode::OneTx => {
                        self.vm_stopped_execution(state, bootloader_state, reason)
                    }
                    VmExecutionMode::Batch => self.vm_finished_execution(state),
                    VmExecutionMode::Bootloader => self.vm_finished_execution(state),
                };
            }
        }
    }
}

impl ResultTracer {
    fn vm_finished_execution<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
    ) {
        let Some(result) = vm_may_have_ended_inner(state) else {
            // The VM has finished execution, but the result is not yet available.
            self.result = Some(Result::Success {
                return_data: vec![],
            });
            return;
        };

        // Check it's not inside tx
        match result {
            VmExecutionResult::Ok(output) => {
                self.result = Some(Result::Success {
                    return_data: output,
                });
            }
            VmExecutionResult::Revert(output) => {
                // Unlike VmHook::ExecutionResult,  vm has completely finished and returned not only the revert reason,
                // but with bytecode, which represents the type of error from the bootloader side
                let revert_reason = TxRevertReason::parse_error(&output);

                match revert_reason {
                    TxRevertReason::TxReverted(reason) => {
                        self.result = Some(Result::Error {
                            error_reason: reason,
                        });
                    }
                    TxRevertReason::Halt(halt) => {
                        self.result = Some(Result::Halt { reason: halt });
                    }
                };
            }
            VmExecutionResult::Panic => {
                if self.bootloader_out_of_gas {
                    self.result = Some(Result::Halt {
                        reason: Halt::BootloaderOutOfGas,
                    });
                } else {
                    self.result = Some(Result::Halt {
                        reason: Halt::VMPanic,
                    });
                }
            }
            VmExecutionResult::MostLikelyDidNotFinish(_, _) => {
                unreachable!()
            }
        }
    }

    fn vm_stopped_execution<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        reason: TracerExecutionStopReason,
    ) {
        if let TracerExecutionStopReason::Abort(halt) = reason {
            self.result = Some(Result::Halt { reason: halt });
            return;
        }

        if self.bootloader_out_of_gas {
            self.result = Some(Result::Halt {
                reason: Halt::BootloaderOutOfGas,
            });
        } else {
            if self.result.is_some() {
                return;
            }

            let has_failed = tx_has_failed(state, bootloader_state.current_tx() as u32);
            if has_failed {
                self.result = Some(Result::Error {
                    error_reason: VmRevertReason::General {
                        msg: "Transaction reverted with empty reason. Possibly out of gas"
                            .to_string(),
                        data: vec![],
                    },
                });
            } else {
                self.result = Some(self.result.clone().unwrap_or(Result::Success {
                    return_data: vec![],
                }));
            }
        }
    }

    pub(crate) fn into_result(self) -> ExecutionResult {
        match self.result.unwrap() {
            Result::Error { error_reason } => ExecutionResult::Revert {
                output: error_reason,
            },
            Result::Success { return_data } => ExecutionResult::Success {
                output: return_data,
            },
            Result::Halt { reason } => ExecutionResult::Halt { reason },
        }
    }
}

pub(crate) fn tx_has_failed<S: WriteStorage, H: HistoryMode>(
    state: &ZkSyncVmState<S, H>,
    tx_id: u32,
) -> bool {
    let mem_slot = RESULT_SUCCESS_FIRST_SLOT + tx_id;
    let mem_value = state
        .memory
        .read_slot(BOOTLOADER_HEAP_PAGE as usize, mem_slot as usize)
        .value;

    mem_value == U256::zero()
}

use std::marker::PhantomData;

use zk_evm_1_5_2::{
    tracing::{AfterDecodingData, BeforeExecutionData, VmLocalStateData},
    vm_state::{ErrorFlags, VmLocalState},
    zkevm_opcode_defs::{FatPointer, Opcode, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER},
};
use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_types::U256;

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStopReason, VmExecutionStopReason},
        ExecutionResult, Halt, TxRevertReason, VmExecutionMode, VmRevertReason,
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{
        bootloader::BootloaderState,
        constants::{get_result_success_first_slot, BOOTLOADER_HEAP_PAGE},
        old_vm::utils::{vm_may_have_ended_inner, VmExecutionResult},
        tracers::{
            traits::VmTracer,
            utils::{get_vm_hook_params, read_pointer},
        },
        types::ZkSyncVmState,
        vm::MultiVmSubversion,
        HistoryMode, SimpleMemory, VmHook,
    },
};

#[derive(Debug, Clone)]
enum Result {
    Error { error_reason: VmRevertReason },
    Success { return_data: Vec<u8> },
    Halt { reason: Halt },
}

/// Responsible for tracing the far calls from the bootloader.
#[derive(Debug, Copy, Clone, Default)]
#[allow(clippy::enum_variant_names)]
enum FarCallTracker {
    #[default]
    NoFarCallObserved,
    FarCallObserved(usize),
    ReturndataObserved(FatPointer),
}

impl FarCallTracker {
    // Should be called before opcode is executed
    fn far_call_observed(&mut self, local_state: &VmLocalStateData<'_>) {
        match &self {
            FarCallTracker::NoFarCallObserved => {
                *self = FarCallTracker::FarCallObserved(
                    local_state.vm_local_state.callstack.inner.len(),
                );
            }
            FarCallTracker::FarCallObserved(_) => {
                panic!("Two far calls from bootloader in a row is not possible")
            }
            FarCallTracker::ReturndataObserved(_) => {
                // Now we forget about the load returndata
                *self = FarCallTracker::FarCallObserved(
                    local_state.vm_local_state.callstack.inner.len(),
                );
            }
        }
    }

    // should be called after opcode is executed
    fn return_observed(&mut self, local_state: &VmLocalStateData<'_>) {
        if let FarCallTracker::FarCallObserved(x) = &self {
            if *x != local_state.vm_local_state.callstack.inner.len() {
                return;
            }

            let returndata_pointer = local_state.vm_local_state.registers
                [RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];

            assert!(
                returndata_pointer.is_pointer,
                "Returndata pointer is not a pointer"
            );

            *self =
                FarCallTracker::ReturndataObserved(FatPointer::from_u256(returndata_pointer.value));
        }
    }

    fn get_latest_returndata(self) -> Option<FatPointer> {
        match self {
            FarCallTracker::ReturndataObserved(x) => Some(x),
            _ => None,
        }
    }
}

/// Tracer responsible for handling the VM execution result.
#[derive(Debug, Clone)]
pub(crate) struct ResultTracer<S> {
    result: Option<Result>,
    bootloader_out_of_gas: bool,
    execution_mode: VmExecutionMode,

    far_call_tracker: FarCallTracker,
    subversion: MultiVmSubversion,

    pub(crate) tx_finished_in_one_tx_mode: bool,

    _phantom: PhantomData<S>,
}

impl<S> ResultTracer<S> {
    pub(crate) fn new(execution_mode: VmExecutionMode, subversion: MultiVmSubversion) -> Self {
        Self {
            result: None,
            bootloader_out_of_gas: false,
            execution_mode,
            far_call_tracker: Default::default(),
            subversion,
            tx_finished_in_one_tx_mode: false,
            _phantom: PhantomData,
        }
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the call stack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for ResultTracer<S> {
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
        let hook = VmHook::from_opcode_memory(&state, &data, self.subversion);
        if matches!(hook, Some(VmHook::PostResult)) {
            let vm_hook_params = get_vm_hook_params(memory, self.subversion);
            let success = vm_hook_params[0];
            let return_ptr = FatPointer::from_u256(vm_hook_params[1]);
            let returndata = read_pointer(memory, return_ptr);
            if success == U256::zero() {
                self.result = Some(Result::Error {
                    // Tx has reverted, without bootloader error, we can simply parse the revert reason
                    error_reason: VmRevertReason::from(returndata.as_slice()),
                });
            } else {
                self.result = Some(Result::Success {
                    return_data: returndata,
                });
            }
        }

        if state.vm_local_state.callstack.current.this_address == BOOTLOADER_ADDRESS {
            let opcode_variant = data.opcode.variant;
            if let Opcode::FarCall(_) = opcode_variant.opcode {
                self.far_call_tracker.far_call_observed(&state);
            }
        }
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: zk_evm_1_5_2::tracing::AfterExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        let opcode_variant = data.opcode.variant;

        if state.vm_local_state.callstack.current.this_address == BOOTLOADER_ADDRESS {
            if let Opcode::Ret(_) = opcode_variant.opcode {
                self.far_call_tracker.return_observed(&state);
            }
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for ResultTracer<S> {
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

impl<S: WriteStorage> ResultTracer<S> {
    fn vm_finished_execution<H: HistoryMode>(&mut self, state: &ZkSyncVmState<S, H>) {
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
                // Unlike `VmHook::ExecutionResult`,  vm has completely finished and returned not only the revert reason,
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

    fn vm_stopped_execution<H: HistoryMode>(
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

            let has_failed =
                tx_has_failed(state, bootloader_state.current_tx() as u32, self.subversion);
            if self.tx_finished_in_one_tx_mode && has_failed {
                self.result = Some(Result::Error {
                    error_reason: VmRevertReason::General {
                        msg: "Transaction reverted with empty reason. Possibly out of gas"
                            .to_string(),
                        data: vec![],
                    },
                });
            } else {
                self.result = Some(Result::Success {
                    return_data: vec![],
                });
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

    pub(crate) fn get_latest_result_ptr(&self) -> Option<FatPointer> {
        self.far_call_tracker.get_latest_returndata()
    }
}

pub(crate) fn tx_has_failed<S: WriteStorage, H: HistoryMode>(
    state: &ZkSyncVmState<S, H>,
    tx_id: u32,
    subversion: MultiVmSubversion,
) -> bool {
    let mem_slot = get_result_success_first_slot(subversion) + tx_id;
    let mem_value = state
        .memory
        .read_slot(BOOTLOADER_HEAP_PAGE as usize, mem_slot as usize)
        .value;

    mem_value == U256::zero()
}

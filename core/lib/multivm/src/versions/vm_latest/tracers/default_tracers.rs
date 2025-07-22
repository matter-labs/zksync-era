use std::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
};

use zk_evm_1_5_2::{
    aux_structures::Timestamp,
    tracing::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    vm_state::VmLocalState,
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{decoding::EncodingModeProduction, Opcode, RetOpcode},
};

use super::{EvmDeployTracer, PubdataTracer};
use crate::{
    glue::GlueInto,
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStatus, TracerExecutionStopReason, VmExecutionStopReason},
        Halt, VmExecutionMode,
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{
        bootloader::{utils::apply_l2_block, BootloaderState},
        constants::BOOTLOADER_HEAP_PAGE,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::{
            dispatcher::TracerDispatcher,
            utils::{computational_gas_price, print_debug_log, print_debug_returndata},
            CircuitsTracer, RefundsTracer, ResultTracer,
        },
        types::ZkSyncVmState,
        vm::MultiVmSubversion,
        VmHook, VmTracer,
    },
};

/// Default tracer for the VM. It manages the other tracers execution and stop the vm when needed.
pub struct DefaultExecutionTracer<S: WriteStorage, H: HistoryMode> {
    tx_has_been_processed: bool,
    execution_mode: VmExecutionMode,

    // Amount of gas used during account validation.
    pub(crate) computational_gas_used: u32,
    // Maximum number of gas that we're allowed to use during account validation.
    tx_validation_gas_limit: u32,
    in_account_validation: bool,
    final_batch_info_requested: bool,
    pub(crate) result_tracer: ResultTracer<S>,
    // This tracer is designed specifically for calculating refunds. Its separation from the custom tracer
    // ensures static dispatch, enhancing performance by avoiding dynamic dispatch overhead.
    // Additionally, being an internal tracer, it saves the results directly to `VmResultAndLogs`.
    pub(crate) refund_tracer: Option<RefundsTracer<S>>,
    // The pubdata tracer is responsible for inserting the pubdata packing information into the bootloader
    // memory at the end of the batch. Its separation from the custom tracer
    // ensures static dispatch, enhancing performance by avoiding dynamic dispatch overhead.
    pub(crate) pubdata_tracer: Option<PubdataTracer<S>>,
    pub(crate) dispatcher: TracerDispatcher<S, H>,
    ret_from_the_bootloader: Option<RetOpcode>,
    // This tracer tracks what opcodes were executed and calculates how much circuits will be generated.
    // It only takes into account circuits that are generated for actual execution. It doesn't
    // take into account e.g circuits produced by the initial bootloader memory commitment.
    pub(crate) circuits_tracer: CircuitsTracer<S, H>,
    // This tracer is responsible for handling EVM deployments and providing the data to the code decommitter.
    pub(crate) evm_deploy_tracer: Option<EvmDeployTracer<S>>,
    subversion: MultiVmSubversion,
    storage: StoragePtr<S>,
    _phantom: PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> DefaultExecutionTracer<S, H> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        computational_gas_limit: u32,
        use_evm_emulator: bool,
        execution_mode: VmExecutionMode,
        dispatcher: TracerDispatcher<S, H>,
        storage: StoragePtr<S>,
        refund_tracer: Option<RefundsTracer<S>>,
        pubdata_tracer: Option<PubdataTracer<S>>,
        subversion: MultiVmSubversion,
    ) -> Self {
        Self {
            tx_has_been_processed: false,
            execution_mode,
            computational_gas_used: 0,
            tx_validation_gas_limit: computational_gas_limit,
            in_account_validation: false,
            final_batch_info_requested: false,
            subversion,
            result_tracer: ResultTracer::new(execution_mode, subversion),
            refund_tracer,
            dispatcher,
            pubdata_tracer,
            ret_from_the_bootloader: None,
            circuits_tracer: CircuitsTracer::new(),
            evm_deploy_tracer: use_evm_emulator.then(EvmDeployTracer::new),
            storage,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn tx_has_been_processed(&self) -> bool {
        self.tx_has_been_processed
    }

    pub(crate) fn validation_run_out_of_gas(&self) -> bool {
        self.computational_gas_used > self.tx_validation_gas_limit
    }

    fn set_fictive_l2_block(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) {
        let current_timestamp = Timestamp(state.local_state.timestamp);
        let new_block_config = bootloader_state.get_new_block_config();
        let subversion = bootloader_state.get_vm_subversion();
        let txs_index = bootloader_state.free_tx_index();
        let l2_block = bootloader_state.insert_fictive_l2_block();
        let mut memory = vec![];
        apply_l2_block(
            &mut memory,
            l2_block,
            txs_index,
            subversion,
            Some(new_block_config),
        );
        state.memory.populate_page(
            BOOTLOADER_HEAP_PAGE as usize,
            memory,
            current_timestamp.glue_into(),
        );
        self.final_batch_info_requested = false;
    }

    fn should_stop_execution(&self) -> TracerExecutionStatus {
        match self.execution_mode {
            VmExecutionMode::OneTx if self.tx_has_been_processed() => {
                return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
            }
            VmExecutionMode::Bootloader if self.ret_from_the_bootloader == Some(RetOpcode::Ok) => {
                return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
            }
            _ => {}
        };
        if self.validation_run_out_of_gas() {
            return TracerExecutionStatus::Stop(TracerExecutionStopReason::Abort(
                Halt::ValidationOutOfGas,
            ));
        }
        TracerExecutionStatus::Continue
    }
}

impl<S: WriteStorage, H: HistoryMode> Debug for DefaultExecutionTracer<S, H> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExecutionTracer").finish()
    }
}

/// The default tracer for the VM manages all other tracers. For the sake of optimization, these tracers are statically dispatched.
/// At the same time, the boilerplate for calling these tracers for all tracer calls is quite extensive.
/// This macro is used to reduce the boilerplate.
///
/// Usage:
/// ```
/// dispatch_tracers!(
///   self.after_decoding(state, data, memory)
/// );
/// ```
/// Whenever a new tracer is added, it should be added to the macro call.
///
/// The macro passes the function call to all tracers.
macro_rules! dispatch_tracers {
    ($self:ident.$function:ident($( $params:expr ),*)) => {
        $self.result_tracer.$function($( $params ),*);
        $self.dispatcher.$function($( $params ),*);
        if let Some(tracer) = &mut $self.refund_tracer {
            tracer.$function($( $params ),*);
        }
        if let Some(tracer) = &mut $self.pubdata_tracer {
            tracer.$function($( $params ),*);
        }
        $self.circuits_tracer.$function($( $params ),*);
        if let Some(tracer) = &mut $self.evm_deploy_tracer {
            tracer.$function($( $params ),*);
        }
    };
}

impl<S: WriteStorage, H: HistoryMode> Tracer for DefaultExecutionTracer<S, H> {
    const CALL_BEFORE_DECODING: bool = false;
    const CALL_AFTER_DECODING: bool = true;
    const CALL_BEFORE_EXECUTION: bool = true;
    const CALL_AFTER_EXECUTION: bool = true;
    type SupportedMemory = SimpleMemory<H>;

    fn before_decoding(
        &mut self,
        _state: VmLocalStateData<'_, 8, EncodingModeProduction>,
        _memory: &Self::SupportedMemory,
    ) {
    }

    fn after_decoding(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterDecodingData,
        memory: &Self::SupportedMemory,
    ) {
        dispatch_tracers!(self.after_decoding(state, data, memory));
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

        let hook = VmHook::from_opcode_memory(&state, &data, self.subversion);
        match hook {
            Some(VmHook::TxHasEnded) if matches!(self.execution_mode, VmExecutionMode::OneTx) => {
                self.result_tracer.tx_finished_in_one_tx_mode = true;
                self.tx_has_been_processed = true;
            }
            Some(VmHook::ValidationExited) => self.in_account_validation = false,
            Some(VmHook::AccountValidationEntered) => self.in_account_validation = true,
            Some(VmHook::FinalBatchInfo) => self.final_batch_info_requested = true,
            Some(VmHook::DebugLog) => print_debug_log(&state, memory, self.subversion),
            Some(VmHook::DebugReturnData) => {
                print_debug_returndata(memory, self.result_tracer.get_latest_result_ptr())
            }
            _ => {}
        }

        dispatch_tracers!(self.before_execution(state, data, memory, self.storage.clone()));
    }

    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        if let VmExecutionMode::Bootloader = self.execution_mode {
            let (next_opcode, _, _) = zk_evm_1_5_2::vm_state::read_and_decode(
                state.vm_local_state,
                memory,
                &mut DummyTracer,
                self,
            );
            if current_frame_is_bootloader(state.vm_local_state) {
                if let Opcode::Ret(ret) = next_opcode.inner.variant.opcode {
                    self.ret_from_the_bootloader = Some(ret);
                }
            }
        }

        dispatch_tracers!(self.after_execution(state, data, memory, self.storage.clone()));
    }
}

impl<S: WriteStorage, H: HistoryMode> DefaultExecutionTracer<S, H> {
    pub(crate) fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        dispatch_tracers!(self.initialize_tracer(state));
    }

    pub(crate) fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        if self.final_batch_info_requested {
            self.set_fictive_l2_block(state, bootloader_state)
        }

        let mut result = self.result_tracer.finish_cycle(state, bootloader_state);
        if let Some(refund_tracer) = &mut self.refund_tracer {
            result = refund_tracer
                .finish_cycle(state, bootloader_state)
                .stricter(&result);
        }
        result = self
            .dispatcher
            .finish_cycle(state, bootloader_state)
            .stricter(&result);
        if let Some(pubdata_tracer) = &mut self.pubdata_tracer {
            result = pubdata_tracer
                .finish_cycle(state, bootloader_state)
                .stricter(&result);
        }

        result = self
            .circuits_tracer
            .finish_cycle(state, bootloader_state)
            .stricter(&result);

        if let Some(evm_deploy_tracer) = &mut self.evm_deploy_tracer {
            result = evm_deploy_tracer
                .finish_cycle(state, bootloader_state)
                .stricter(&result);
        }

        result.stricter(&self.should_stop_execution())
    }

    pub(crate) fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &BootloaderState,
        stop_reason: VmExecutionStopReason,
    ) {
        dispatch_tracers!(self.after_vm_execution(state, bootloader_state, stop_reason.clone()));
    }
}

fn current_frame_is_bootloader(local_state: &VmLocalState) -> bool {
    // The current frame is bootloader if the call stack depth is 1.
    // Some of the near calls inside the bootloader can be out of gas, which is totally normal behavior
    // and it shouldn't result in `is_bootloader_out_of_gas` becoming true.
    local_state.callstack.inner.len() == 1
}

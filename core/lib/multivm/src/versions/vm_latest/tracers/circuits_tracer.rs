use std::marker::PhantomData;

use zk_evm_1_4_0::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{LogOpcode, Opcode, UMAOpcode},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_system_constants::{
    ECRECOVER_PRECOMPILE_ADDRESS, KECCAK256_PRECOMPILE_ADDRESS, SHA256_PRECOMPILE_ADDRESS,
};

use super::circuits_capacity::*;
use crate::{
    interface::{dyn_tracers::vm_1_4_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        bootloader_state::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::traits::VmTracer,
        types::internals::ZkSyncVmState,
    },
};

/// Tracer responsible for collecting information about refunds.
#[derive(Debug)]
pub(crate) struct CircuitsTracer<S> {
    pub(crate) estimated_circuits_used: f32,
    last_decommitment_history_entry_checked: Option<usize>,
    last_hot_writes_history_entry_checked: Option<usize>,
    last_hot_reads_history_entry_checked: Option<usize>,
    _phantom_data: PhantomData<S>,
}

impl<S: WriteStorage> CircuitsTracer<S> {
    pub(crate) fn new() -> Self {
        Self {
            estimated_circuits_used: 0.0,
            last_decommitment_history_entry_checked: None,
            last_hot_writes_history_entry_checked: None,
            last_hot_reads_history_entry_checked: None,
            _phantom_data: Default::default(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CircuitsTracer<S> {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        let used = match data.opcode.variant.opcode {
            Opcode::Nop(_)
            | Opcode::Add(_)
            | Opcode::Sub(_)
            | Opcode::Mul(_)
            | Opcode::Div(_)
            | Opcode::Jump(_)
            | Opcode::Binop(_)
            | Opcode::Shift(_)
            | Opcode::Ptr(_) => RICH_ADDRESSING_OPCODE_FRACTION,
            Opcode::Context(_) | Opcode::Ret(_) | Opcode::NearCall(_) => AVERAGE_OPCODE_FRACTION,
            Opcode::Log(LogOpcode::StorageRead) => STORAGE_READ_BASE_FRACTION,
            Opcode::Log(LogOpcode::StorageWrite) => STORAGE_WRITE_BASE_FRACTION,
            Opcode::Log(LogOpcode::ToL1Message) | Opcode::Log(LogOpcode::Event) => {
                EVENT_OR_L1_MESSAGE_FRACTION
            }
            Opcode::Log(LogOpcode::PrecompileCall) => {
                match state.vm_local_state.callstack.current.this_address {
                    a if a == KECCAK256_PRECOMPILE_ADDRESS => {
                        let precompile_interpreted = data.src0_value.value.0[3] as f32;
                        PRECOMPILE_CALL_COMMON_FRACTION
                            + precompile_interpreted * KECCAK256_CYCLE_FRACTION
                    }
                    a if a == SHA256_PRECOMPILE_ADDRESS => {
                        let precompile_interpreted = data.src0_value.value.0[3] as f32;
                        PRECOMPILE_CALL_COMMON_FRACTION
                            + precompile_interpreted * SHA256_CYCLE_FRACTION
                    }
                    a if a == ECRECOVER_PRECOMPILE_ADDRESS => {
                        PRECOMPILE_CALL_COMMON_FRACTION + ECRECOVER_CYCLE_FRACTION
                    }
                    _ => PRECOMPILE_CALL_COMMON_FRACTION,
                }
            }
            Opcode::FarCall(_) => FAR_CALL_FRACTION,
            Opcode::UMA(UMAOpcode::AuxHeapWrite | UMAOpcode::HeapWrite) => UMA_WRITE_FRACTION,
            Opcode::UMA(
                UMAOpcode::AuxHeapRead | UMAOpcode::HeapRead | UMAOpcode::FatPointerRead,
            ) => UMA_READ_FRACTION,
            Opcode::Invalid(_) => unreachable!(), // invalid opcodes are never executed
        };

        self.estimated_circuits_used += used;
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CircuitsTracer<S> {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.last_decommitment_history_entry_checked = Some(
            state
                .decommittment_processor
                .decommitted_code_hashes
                .history()
                .len(),
        );

        self.last_hot_writes_history_entry_checked = Some(state.storage.hot_writes.history().len());

        self.last_hot_reads_history_entry_checked = Some(state.storage.hot_reads.history().len());
    }

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        // Trace decommitments.
        let last_decommitment_history_entry_checked = self
            .last_decommitment_history_entry_checked
            .expect("Value must be set during init");
        let history = state
            .decommittment_processor
            .decommitted_code_hashes
            .history();
        for (_, history_event) in &history[last_decommitment_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_some());
            let bytecode_len = state
                .decommittment_processor
                .known_bytecodes
                .inner()
                .get(&history_event.key)
                .expect("Bytecode must be known at this point")
                .len();

            // Each cycle of `CodeDecommitter` processes 64 bits.
            let decommitter_cycles_used = bytecode_len / 4;
            self.estimated_circuits_used +=
                (decommitter_cycles_used as f32) * CODE_DECOMMITTER_CYCLE_FRACTION;
        }
        self.last_decommitment_history_entry_checked = Some(history.len());

        // Process storage writes
        let last_writes_history_entry_checked = self
            .last_hot_writes_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.hot_writes.history();
        for (_, history_event) in &history[last_writes_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_some());

            self.estimated_circuits_used += 2.0 * STORAGE_APPLICATION_CYCLE_FRACTION;
        }
        self.last_hot_writes_history_entry_checked = Some(history.len());

        // Process storage reads
        let last_reads_history_entry_checked = self
            .last_hot_reads_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.hot_reads.history();
        for (_, history_event) in &history[last_reads_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_some());

            // If the slot was already written to, then we already take 2 cycles into account.
            if !state
                .storage
                .hot_writes
                .inner()
                .contains_key(&history_event.key)
            {
                self.estimated_circuits_used += STORAGE_APPLICATION_CYCLE_FRACTION;
            }
        }
        self.last_hot_reads_history_entry_checked = Some(history.len());

        TracerExecutionStatus::Continue
    }
}

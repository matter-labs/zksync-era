use std::marker::PhantomData;

use zk_evm_1_4_1::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zk_evm_abstractions::precompiles::PrecompileAddress,
    zkevm_opcode_defs::{LogOpcode, Opcode, UMAOpcode},
};
use zksync_state::{StoragePtr, WriteStorage};

use super::circuits_capacity::*;
use crate::{
    interface::{dyn_tracers::vm_1_4_1::DynTracer, tracer::TracerExecutionStatus},
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
    last_written_keys_history_entry_checked: Option<usize>,
    last_read_keys_history_entry_checked: Option<usize>,
    last_precompile_inner_entry_checked: Option<usize>,
    _phantom_data: PhantomData<S>,
}

impl<S: WriteStorage> CircuitsTracer<S> {
    pub(crate) fn new() -> Self {
        Self {
            estimated_circuits_used: 0.0,
            last_decommitment_history_entry_checked: None,
            last_written_keys_history_entry_checked: None,
            last_read_keys_history_entry_checked: None,
            last_precompile_inner_entry_checked: None,
            _phantom_data: Default::default(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CircuitsTracer<S> {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
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
            Opcode::Log(LogOpcode::PrecompileCall) => PRECOMPILE_CALL_COMMON_FRACTION,
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

        self.last_written_keys_history_entry_checked =
            Some(state.storage.written_keys.history().len());

        self.last_read_keys_history_entry_checked = Some(state.storage.read_keys.history().len());

        self.last_precompile_inner_entry_checked = Some(
            state
                .precompiles_processor
                .precompile_cycles_history
                .inner()
                .len(),
        );
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
            assert!(history_event.value.is_none());
            let bytecode_len = state
                .decommittment_processor
                .known_bytecodes
                .inner()
                .get(&history_event.key)
                .expect("Bytecode must be known at this point")
                .len();

            // Each cycle of `CodeDecommitter` processes 2 words.
            // If the number of words in bytecode is odd, then number of cycles must be rounded up.
            let decommitter_cycles_used = (bytecode_len + 1) / 2;
            self.estimated_circuits_used +=
                (decommitter_cycles_used as f32) * CODE_DECOMMITTER_CYCLE_FRACTION;
        }
        self.last_decommitment_history_entry_checked = Some(history.len());

        // Process storage writes.
        let last_writes_history_entry_checked = self
            .last_written_keys_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.written_keys.history();
        for (_, history_event) in &history[last_writes_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_none());

            self.estimated_circuits_used += 2.0 * STORAGE_APPLICATION_CYCLE_FRACTION;
        }
        self.last_written_keys_history_entry_checked = Some(history.len());

        // Process storage reads.
        let last_reads_history_entry_checked = self
            .last_read_keys_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.read_keys.history();
        for (_, history_event) in &history[last_reads_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_none());

            // If the slot is already written to, then we've already taken 2 cycles into account.
            if !state
                .storage
                .written_keys
                .inner()
                .contains_key(&history_event.key)
            {
                self.estimated_circuits_used += STORAGE_APPLICATION_CYCLE_FRACTION;
            }
        }
        self.last_read_keys_history_entry_checked = Some(history.len());

        // Process precompiles.
        let last_precompile_inner_entry_checked = self
            .last_precompile_inner_entry_checked
            .expect("Value must be set during init");
        let inner = state
            .precompiles_processor
            .precompile_cycles_history
            .inner();
        for (precompile, cycles) in &inner[last_precompile_inner_entry_checked..] {
            let fraction = match precompile {
                PrecompileAddress::Ecrecover => ECRECOVER_CYCLE_FRACTION,
                PrecompileAddress::SHA256 => SHA256_CYCLE_FRACTION,
                PrecompileAddress::Keccak256 => KECCAK256_CYCLE_FRACTION,
            };
            self.estimated_circuits_used += (*cycles as f32) * fraction;
        }
        self.last_precompile_inner_entry_checked = Some(inner.len());

        TracerExecutionStatus::Continue
    }
}

use std::marker::PhantomData;

use zk_evm_1_4_1::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zk_evm_abstractions::precompiles::PrecompileAddress,
    zkevm_opcode_defs::{LogOpcode, Opcode, UMAOpcode},
};

use super::circuits_capacity::*;
use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::TracerExecutionStatus,
    },
    tracers::dynamic::vm_1_4_1::DynTracer,
    utils::CircuitCycleStatistic,
    vm_1_4_2::{
        bootloader_state::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::traits::VmTracer,
        types::internals::ZkSyncVmState,
    },
};

/// Tracer responsible for collecting information about refunds.
#[derive(Debug)]
pub(crate) struct CircuitsTracer<S, H> {
    pub(crate) statistics: CircuitCycleStatistic,
    last_decommitment_history_entry_checked: Option<usize>,
    last_written_keys_history_entry_checked: Option<usize>,
    last_read_keys_history_entry_checked: Option<usize>,
    last_precompile_inner_entry_checked: Option<usize>,
    _phantom_data: PhantomData<(S, H)>,
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CircuitsTracer<S, H> {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        self.statistics.main_vm_cycles += 1;

        match data.opcode.variant.opcode {
            Opcode::Nop(_)
            | Opcode::Add(_)
            | Opcode::Sub(_)
            | Opcode::Mul(_)
            | Opcode::Div(_)
            | Opcode::Jump(_)
            | Opcode::Binop(_)
            | Opcode::Shift(_)
            | Opcode::Ptr(_) => {
                self.statistics.ram_permutation_cycles += RICH_ADDRESSING_OPCODE_RAM_CYCLES;
            }
            Opcode::Context(_) | Opcode::Ret(_) | Opcode::NearCall(_) => {
                self.statistics.ram_permutation_cycles += AVERAGE_OPCODE_RAM_CYCLES;
            }
            Opcode::Log(LogOpcode::StorageRead) => {
                self.statistics.ram_permutation_cycles += STORAGE_READ_RAM_CYCLES;
                self.statistics.log_demuxer_cycles += STORAGE_READ_LOG_DEMUXER_CYCLES;
                self.statistics.storage_sorter_cycles += STORAGE_READ_STORAGE_SORTER_CYCLES;
            }
            Opcode::Log(LogOpcode::StorageWrite) => {
                self.statistics.ram_permutation_cycles += STORAGE_WRITE_RAM_CYCLES;
                self.statistics.log_demuxer_cycles += STORAGE_WRITE_LOG_DEMUXER_CYCLES;
                self.statistics.storage_sorter_cycles += STORAGE_WRITE_STORAGE_SORTER_CYCLES;
            }
            Opcode::Log(LogOpcode::ToL1Message) | Opcode::Log(LogOpcode::Event) => {
                self.statistics.ram_permutation_cycles += EVENT_RAM_CYCLES;
                self.statistics.log_demuxer_cycles += EVENT_LOG_DEMUXER_CYCLES;
                self.statistics.events_sorter_cycles += EVENT_EVENTS_SORTER_CYCLES;
            }
            Opcode::Log(LogOpcode::PrecompileCall) => {
                self.statistics.ram_permutation_cycles += PRECOMPILE_RAM_CYCLES;
                self.statistics.log_demuxer_cycles += PRECOMPILE_LOG_DEMUXER_CYCLES;
            }
            Opcode::FarCall(_) => {
                self.statistics.ram_permutation_cycles += FAR_CALL_RAM_CYCLES;
                self.statistics.code_decommitter_sorter_cycles +=
                    FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES;
                self.statistics.storage_sorter_cycles += FAR_CALL_STORAGE_SORTER_CYCLES;
                self.statistics.log_demuxer_cycles += FAR_CALL_LOG_DEMUXER_CYCLES;
            }
            Opcode::UMA(UMAOpcode::AuxHeapWrite | UMAOpcode::HeapWrite) => {
                self.statistics.ram_permutation_cycles += UMA_WRITE_RAM_CYCLES;
            }
            Opcode::UMA(
                UMAOpcode::AuxHeapRead | UMAOpcode::HeapRead | UMAOpcode::FatPointerRead,
            ) => {
                self.statistics.ram_permutation_cycles += UMA_READ_RAM_CYCLES;
            }
            Opcode::Invalid(_) => unreachable!(), // invalid opcodes are never executed
        };
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CircuitsTracer<S, H> {
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
        self.trace_decommitments(state);
        self.trace_storage_writes(state);
        self.trace_storage_reads(state);
        self.trace_precompile_calls(state);

        TracerExecutionStatus::Continue
    }
}

impl<S: WriteStorage, H: HistoryMode> CircuitsTracer<S, H> {
    pub(crate) fn new() -> Self {
        Self {
            statistics: CircuitCycleStatistic::default(),
            last_decommitment_history_entry_checked: None,
            last_written_keys_history_entry_checked: None,
            last_read_keys_history_entry_checked: None,
            last_precompile_inner_entry_checked: None,
            _phantom_data: Default::default(),
        }
    }

    fn trace_decommitments(&mut self, state: &ZkSyncVmState<S, H>) {
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
            let decommitter_cycles_used = bytecode_len.div_ceil(2);
            self.statistics.code_decommitter_cycles += decommitter_cycles_used as u32;
        }
        self.last_decommitment_history_entry_checked = Some(history.len());
    }

    fn trace_storage_writes(&mut self, state: &ZkSyncVmState<S, H>) {
        let last_writes_history_entry_checked = self
            .last_written_keys_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.written_keys.history();
        for (_, history_event) in &history[last_writes_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_none());

            self.statistics.storage_application_cycles += STORAGE_WRITE_STORAGE_APPLICATION_CYCLES;
        }
        self.last_written_keys_history_entry_checked = Some(history.len());
    }

    fn trace_storage_reads(&mut self, state: &ZkSyncVmState<S, H>) {
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
                self.statistics.storage_application_cycles +=
                    STORAGE_READ_STORAGE_APPLICATION_CYCLES;
            }
        }
        self.last_read_keys_history_entry_checked = Some(history.len());
    }

    fn trace_precompile_calls(&mut self, state: &ZkSyncVmState<S, H>) {
        let last_precompile_inner_entry_checked = self
            .last_precompile_inner_entry_checked
            .expect("Value must be set during init");
        let inner = state
            .precompiles_processor
            .precompile_cycles_history
            .inner();
        for (precompile, cycles) in &inner[last_precompile_inner_entry_checked..] {
            match precompile {
                PrecompileAddress::Ecrecover => {
                    self.statistics.ecrecover_cycles += *cycles as u32;
                }
                PrecompileAddress::SHA256 => {
                    self.statistics.sha256_cycles += *cycles as u32;
                }
                PrecompileAddress::Keccak256 => {
                    self.statistics.keccak256_cycles += *cycles as u32;
                }
            };
        }
        self.last_precompile_inner_entry_checked = Some(inner.len());
    }
}

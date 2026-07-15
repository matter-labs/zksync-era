use zk_evm_1_5_2::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zk_evm_abstractions::precompiles::PrecompileAddress,
    zkevm_opcode_defs::{LogOpcode, Opcode, UMAOpcode},
};

use super::{CycleFeatureTracer, FeatureId};
use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::{TracerExecutionStatus, VmExecutionStopReason},
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    vm_latest::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CycleFeatureTracer {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        // Opcode → feature-family bucketing mirrors `circuits_tracer` (and the fast-VM
        // tracer), so the legacy VM emits the same feature categories the model expects.
        let id = match data.opcode.variant.opcode {
            Opcode::Nop(_)
            | Opcode::Add(_)
            | Opcode::Sub(_)
            | Opcode::Mul(_)
            | Opcode::Div(_)
            | Opcode::Jump(_)
            | Opcode::Binop(_)
            | Opcode::Shift(_)
            | Opcode::Ptr(_) => FeatureId::RichAddressingOp,
            Opcode::Context(_) | Opcode::Ret(_) => FeatureId::AverageOp,
            Opcode::NearCall(_) => {
                // Near calls are an `AverageOp` for cycle purposes but also a
                // distinct batch feature (matching the fast-VM tracer).
                self.bump(FeatureId::NearCallCount, 1);
                FeatureId::AverageOp
            }
            Opcode::Log(LogOpcode::StorageRead) => FeatureId::StorageRead,
            Opcode::Log(LogOpcode::TransientStorageRead) => FeatureId::TransientStorageRead,
            Opcode::Log(LogOpcode::StorageWrite) => FeatureId::StorageWrite,
            Opcode::Log(LogOpcode::TransientStorageWrite) => FeatureId::TransientStorageWrite,
            Opcode::Log(LogOpcode::ToL1Message) | Opcode::Log(LogOpcode::Event) => FeatureId::Event,
            Opcode::Log(LogOpcode::PrecompileCall) => FeatureId::PrecompileCall,
            Opcode::Log(LogOpcode::Decommit) => FeatureId::Decommit,
            Opcode::FarCall(_) => FeatureId::FarCall,
            Opcode::UMA(
                UMAOpcode::AuxHeapWrite | UMAOpcode::HeapWrite | UMAOpcode::StaticMemoryWrite,
            ) => FeatureId::UmaWrite,
            Opcode::UMA(
                UMAOpcode::AuxHeapRead
                | UMAOpcode::HeapRead
                | UMAOpcode::FatPointerRead
                | UMAOpcode::StaticMemoryRead,
            ) => FeatureId::UmaRead,
            Opcode::Invalid(_) => unreachable!(), // invalid opcodes are never executed
        };
        self.bump(id, 1);
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CycleFeatureTracer {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.last_decommitment_history_entry_checked = Some(
            state
                .decommittment_processor
                .decommitted_code_hashes
                .history()
                .len(),
        );
        self.last_written_keys_history_entry_checked =
            Some(state.storage.written_storage_keys.history().len());
        self.last_read_keys_history_entry_checked =
            Some(state.storage.read_storage_keys.history().len());
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
        // Complexity features (decommit words, storage applications, precompile rounds)
        // aren't observable from opcodes; recover them by scanning the VM history logs.
        self.trace_decommitments(state);
        self.trace_storage_writes(state);
        self.trace_storage_reads(state);
        self.trace_precompile_calls(state);

        TracerExecutionStatus::Continue
    }

    fn after_vm_execution(
        &mut self,
        _state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
        // Publish the final feature vector for handle-only callers. Ignore a
        // second set: a dispatcher may run several segments against clones.
        let _ = self.result.set(self.features.clone());
    }
}

impl CycleFeatureTracer {
    fn trace_decommitments<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
    ) {
        let last_checked = self
            .last_decommitment_history_entry_checked
            .expect("Value must be set during init");
        let history = state
            .decommittment_processor
            .decommitted_code_hashes
            .history();
        for (_, history_event) in &history[last_checked..] {
            // Cycles are charged once per bytecode, when it is actually decommitted.
            if history_event.value.is_some() {
                let bytecode_len = state
                    .decommittment_processor
                    .known_bytecodes
                    .inner()
                    .get(&history_event.key)
                    .expect("Bytecode must be known at this point")
                    .len();
                // `CodeDecommitter` processes 2 words per cycle (round up for odd).
                // This is the `CycleStats::Decommit` quantity in the fast VM.
                self.bump(FeatureId::DecommitCycles, bytecode_len.div_ceil(2) as u64);
            }
        }
        self.last_decommitment_history_entry_checked = Some(history.len());
    }

    fn trace_storage_writes<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
    ) {
        let last_checked = self
            .last_written_keys_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.written_storage_keys.history();
        for (_, history_event) in &history[last_checked..] {
            // Only insertions happen during a single VM inspection.
            assert!(history_event.value.is_none());
            // A new written slot costs 2 storage-application cycles
            // (`CycleStats::StorageWrite` in the fast VM).
            self.bump(FeatureId::StorageApplication, 2);
        }
        self.last_written_keys_history_entry_checked = Some(history.len());
    }

    fn trace_storage_reads<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
    ) {
        let last_checked = self
            .last_read_keys_history_entry_checked
            .expect("Value must be set during init");
        let history = state.storage.read_storage_keys.history();
        for (_, history_event) in &history[last_checked..] {
            assert!(history_event.value.is_none());
            // A slot already written was counted (2 cycles) above; a fresh read
            // slot costs 1 (`CycleStats::StorageRead` in the fast VM).
            if !state
                .storage
                .written_storage_keys
                .inner()
                .contains_key(&history_event.key)
            {
                self.bump(FeatureId::StorageApplication, 1);
            }
        }
        self.last_read_keys_history_entry_checked = Some(history.len());
    }

    fn trace_precompile_calls<S: WriteStorage, H: HistoryMode>(
        &mut self,
        state: &ZkSyncVmState<S, H>,
    ) {
        let last_checked = self
            .last_precompile_inner_entry_checked
            .expect("Value must be set during init");
        let inner = state
            .precompiles_processor
            .precompile_cycles_history
            .inner();
        for (precompile, cycles) in &inner[last_checked..] {
            // Value = operation complexity (hashing rounds / circuit cycles), so it
            // already scales with input size — same as the fast-VM `CycleStats`.
            let cycles = *cycles as u64;
            let id = match precompile {
                PrecompileAddress::Ecrecover => FeatureId::EcRecoverCycles,
                PrecompileAddress::SHA256 => FeatureId::Sha256Cycles,
                PrecompileAddress::Keccak256 => FeatureId::Keccak256Cycles,
                PrecompileAddress::Secp256r1Verify => FeatureId::Secp256r1VerifyCycles,
                PrecompileAddress::Modexp => FeatureId::ModExpCycles,
                PrecompileAddress::ECAdd => FeatureId::EcAddCycles,
                PrecompileAddress::ECMul => FeatureId::EcMulCycles,
                PrecompileAddress::ECPairing => FeatureId::EcPairingCycles,
            };
            self.bump(id, cycles);
        }
        self.last_precompile_inner_entry_checked = Some(inner.len());
    }
}

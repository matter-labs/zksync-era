use circuit_sequencer_api::geometry_config::{GeometryConfig, ProtocolGeometry};
use zksync_vm2::interface::{
    CycleStats, GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer,
};
use zksync_vm_interface::CircuitStatistic;

use crate::vm_latest::tracers::circuits_capacity::*;

/// VM tracer tracking [`CircuitStatistic`]s. Statistics generally depend on the number of time some opcodes were invoked,
/// and, for precompiles, invocation complexity (e.g., how many hashing cycles `keccak256` required).
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct CircuitsTracer {
    main_vm_cycles: u32,
    ram_permutation_cycles: u32,
    storage_application_cycles: u32,
    storage_sorter_cycles: u32,
    code_decommitter_cycles: u32,
    code_decommitter_sorter_cycles: u32,
    log_demuxer_cycles: u32,
    events_sorter_cycles: u32,
    keccak256_cycles: u32,
    ecrecover_cycles: u32,
    sha256_cycles: u32,
    secp256r1_verify_cycles: u32,
    transient_storage_checker_cycles: u32,
    modexp_cycles: u32,
    ecadd_cycles: u32,
    ecmul_cycles: u32,
    ecpairing_cycles: u32,
}

impl Tracer for CircuitsTracer {
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        _: &mut S,
    ) -> ShouldStop {
        self.main_vm_cycles += 1;

        match OP::VALUE {
            Opcode::Nop
            | Opcode::Add
            | Opcode::Sub
            | Opcode::Mul
            | Opcode::Div
            | Opcode::Jump
            | Opcode::Xor
            | Opcode::And
            | Opcode::Or
            | Opcode::ShiftLeft
            | Opcode::ShiftRight
            | Opcode::RotateLeft
            | Opcode::RotateRight
            | Opcode::PointerAdd
            | Opcode::PointerSub
            | Opcode::PointerPack
            | Opcode::PointerShrink => {
                self.ram_permutation_cycles += RICH_ADDRESSING_OPCODE_RAM_CYCLES;
            }
            Opcode::This
            | Opcode::Caller
            | Opcode::CodeAddress
            | Opcode::ContextMeta
            | Opcode::ErgsLeft
            | Opcode::SP
            | Opcode::ContextU128
            | Opcode::SetContextU128
            | Opcode::AuxMutating0
            | Opcode::IncrementTxNumber
            | Opcode::Ret(_)
            | Opcode::NearCall => {
                self.ram_permutation_cycles += AVERAGE_OPCODE_RAM_CYCLES;
            }
            Opcode::StorageRead => {
                self.ram_permutation_cycles += STORAGE_READ_RAM_CYCLES;
                self.log_demuxer_cycles += STORAGE_READ_LOG_DEMUXER_CYCLES;
                self.storage_sorter_cycles += STORAGE_READ_STORAGE_SORTER_CYCLES;
            }
            Opcode::TransientStorageRead => {
                self.ram_permutation_cycles += TRANSIENT_STORAGE_READ_RAM_CYCLES;
                self.log_demuxer_cycles += TRANSIENT_STORAGE_READ_LOG_DEMUXER_CYCLES;
                self.transient_storage_checker_cycles +=
                    TRANSIENT_STORAGE_READ_TRANSIENT_STORAGE_CHECKER_CYCLES;
            }
            Opcode::StorageWrite => {
                self.ram_permutation_cycles += STORAGE_WRITE_RAM_CYCLES;
                self.log_demuxer_cycles += STORAGE_WRITE_LOG_DEMUXER_CYCLES;
                self.storage_sorter_cycles += STORAGE_WRITE_STORAGE_SORTER_CYCLES;
            }
            Opcode::TransientStorageWrite => {
                self.ram_permutation_cycles += TRANSIENT_STORAGE_WRITE_RAM_CYCLES;
                self.log_demuxer_cycles += TRANSIENT_STORAGE_WRITE_LOG_DEMUXER_CYCLES;
                self.transient_storage_checker_cycles +=
                    TRANSIENT_STORAGE_WRITE_TRANSIENT_STORAGE_CHECKER_CYCLES;
            }
            Opcode::L2ToL1Message | Opcode::Event => {
                self.ram_permutation_cycles += EVENT_RAM_CYCLES;
                self.log_demuxer_cycles += EVENT_LOG_DEMUXER_CYCLES;
                self.events_sorter_cycles += EVENT_EVENTS_SORTER_CYCLES;
            }
            Opcode::PrecompileCall => {
                self.ram_permutation_cycles += PRECOMPILE_RAM_CYCLES;
                self.log_demuxer_cycles += PRECOMPILE_LOG_DEMUXER_CYCLES;
            }
            Opcode::Decommit => {
                // Note, that for decommit the log demuxer circuit is not used.
                self.ram_permutation_cycles += LOG_DECOMMIT_RAM_CYCLES;
                self.code_decommitter_sorter_cycles += LOG_DECOMMIT_DECOMMITTER_SORTER_CYCLES;
            }
            Opcode::FarCall(_) => {
                self.ram_permutation_cycles += FAR_CALL_RAM_CYCLES;
                self.code_decommitter_sorter_cycles += FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES;
                self.storage_sorter_cycles += FAR_CALL_STORAGE_SORTER_CYCLES;
                self.log_demuxer_cycles += FAR_CALL_LOG_DEMUXER_CYCLES;
            }
            Opcode::AuxHeapWrite | Opcode::HeapWrite /* StaticMemoryWrite */ => {
                self.ram_permutation_cycles += UMA_WRITE_RAM_CYCLES;
            }
            Opcode::AuxHeapRead | Opcode::HeapRead | Opcode::PointerRead /* StaticMemoryRead */ => {
                self.ram_permutation_cycles += UMA_READ_RAM_CYCLES;
            }
        }

        ShouldStop::Continue
    }

    fn on_extra_prover_cycles(&mut self, stats: CycleStats) {
        match stats {
            CycleStats::Keccak256(cycles) => self.keccak256_cycles += cycles,
            CycleStats::Sha256(cycles) => self.sha256_cycles += cycles,
            CycleStats::EcRecover(cycles) => self.ecrecover_cycles += cycles,
            CycleStats::Secp256r1Verify(cycles) => self.secp256r1_verify_cycles += cycles,
            CycleStats::Decommit(cycles) => self.code_decommitter_cycles += cycles,
            CycleStats::StorageRead => self.storage_application_cycles += 1,
            CycleStats::StorageWrite => self.storage_application_cycles += 2,
            CycleStats::EcAdd(cycles) => self.ecadd_cycles += cycles,
            CycleStats::ModExp(cycles) => self.modexp_cycles += cycles,
            CycleStats::EcMul(cycles) => self.ecmul_cycles += cycles,
            CycleStats::EcPairing(cycles) => self.ecpairing_cycles += cycles,
        }
    }
}

impl CircuitsTracer {
    /// Obtains the current circuit stats from this tracer.
    pub fn circuit_statistic(&self) -> CircuitStatistic {
        CircuitStatistic {
            main_vm: self.main_vm_cycles as f32 / GEOMETRY_CONFIG.cycles_per_vm_snapshot as f32,
            ram_permutation: self.ram_permutation_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_ram_permutation as f32,
            storage_application: self.storage_application_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_storage_application as f32,
            storage_sorter: self.storage_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_storage_sorter as f32,
            code_decommitter: self.code_decommitter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_code_decommitter as f32,
            code_decommitter_sorter: self.code_decommitter_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_code_decommitter_sorter as f32,
            log_demuxer: self.log_demuxer_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_log_demuxer as f32,
            events_sorter: self.events_sorter_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_events_or_l1_messages_sorter as f32,
            keccak256: self.keccak256_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_keccak256_circuit as f32,
            ecrecover: self.ecrecover_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_ecrecover_circuit as f32,
            sha256: self.sha256_cycles as f32 / GEOMETRY_CONFIG.cycles_per_sha256_circuit as f32,
            secp256k1_verify: self.secp256r1_verify_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_secp256r1_verify_circuit as f32,
            transient_storage_checker: self.transient_storage_checker_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_transient_storage_sorter as f32,
            modexp: self.modexp_cycles as f32 / GEOMETRY_CONFIG.cycles_per_modexp_circuit as f32,
            ecadd: self.ecadd_cycles as f32 / GEOMETRY_CONFIG.cycles_per_ecadd_circuit as f32,
            ecmul: self.ecmul_cycles as f32 / GEOMETRY_CONFIG.cycles_per_ecmul_circuit as f32,
            ecpairing: self.ecpairing_cycles as f32
                / GEOMETRY_CONFIG.cycles_per_ecpairing_circuit as f32,
        }
    }
}

const GEOMETRY_CONFIG: GeometryConfig = ProtocolGeometry::latest().config();

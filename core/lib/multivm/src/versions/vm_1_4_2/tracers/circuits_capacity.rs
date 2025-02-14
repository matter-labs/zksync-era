use circuit_sequencer_api::geometry_config::{GeometryConfig, ProtocolGeometry};

use crate::{interface::CircuitStatistic, utils::CircuitCycleStatistic};

// "Rich addressing" opcodes are opcodes that can write their return value/read the input onto the stack
// and so take 1-2 RAM permutations more than an average opcode.
// In the worst case, a rich addressing may take 3 ram permutations
// (1 for reading the opcode, 1 for writing input value, 1 for writing output value).
pub(crate) const RICH_ADDRESSING_OPCODE_RAM_CYCLES: u32 = 3;

pub(crate) const AVERAGE_OPCODE_RAM_CYCLES: u32 = 1;

pub(crate) const STORAGE_READ_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_APPLICATION_CYCLES: u32 = 1;

pub(crate) const EVENT_RAM_CYCLES: u32 = 1;
pub(crate) const EVENT_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const EVENT_EVENTS_SORTER_CYCLES: u32 = 2;

pub(crate) const STORAGE_WRITE_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_WRITE_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_SORTER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_APPLICATION_CYCLES: u32 = 2;

pub(crate) const FAR_CALL_RAM_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_LOG_DEMUXER_CYCLES: u32 = 1;

// 5 RAM permutations, because: 1 to read opcode + 2 reads + 2 writes.
// 2 reads and 2 writes are needed because unaligned access is implemented with
// aligned queries.
pub(crate) const UMA_WRITE_RAM_CYCLES: u32 = 5;

// 3 RAM permutations, because: 1 to read opcode + 2 reads.
// 2 reads are needed because unaligned access is implemented with aligned queries.
pub(crate) const UMA_READ_RAM_CYCLES: u32 = 3;

pub(crate) const PRECOMPILE_RAM_CYCLES: u32 = 1;
pub(crate) const PRECOMPILE_LOG_DEMUXER_CYCLES: u32 = 1;

const GEOMETRY_CONFIG: GeometryConfig = ProtocolGeometry::V1_4_2.config();

pub(crate) fn circuit_statistic_from_cycles(cycles: CircuitCycleStatistic) -> CircuitStatistic {
    CircuitStatistic {
        main_vm: cycles.main_vm_cycles as f32 / GEOMETRY_CONFIG.cycles_per_vm_snapshot as f32,
        ram_permutation: cycles.ram_permutation_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_ram_permutation as f32,
        storage_application: cycles.storage_application_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_storage_application as f32,
        storage_sorter: cycles.storage_sorter_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_storage_sorter as f32,
        code_decommitter: cycles.code_decommitter_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_code_decommitter as f32,
        code_decommitter_sorter: cycles.code_decommitter_sorter_cycles as f32
            / GEOMETRY_CONFIG.cycles_code_decommitter_sorter as f32,
        log_demuxer: cycles.log_demuxer_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_log_demuxer as f32,
        events_sorter: cycles.events_sorter_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_events_or_l1_messages_sorter as f32,
        keccak256: cycles.keccak256_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_keccak256_circuit as f32,
        ecrecover: cycles.ecrecover_cycles as f32
            / GEOMETRY_CONFIG.cycles_per_ecrecover_circuit as f32,
        sha256: cycles.sha256_cycles as f32 / GEOMETRY_CONFIG.cycles_per_sha256_circuit as f32,
        secp256k1_verify: 0.0,
        transient_storage_checker: 0.0,
        ..Default::default()
    }
}

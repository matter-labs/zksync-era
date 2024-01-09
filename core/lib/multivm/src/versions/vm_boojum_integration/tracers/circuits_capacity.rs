use zkevm_test_harness_1_4_0::{geometry_config::get_geometry_config, toolset::GeometryConfig};

pub(crate) const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();
pub(crate) const OVERESTIMATE_PERCENT: f32 = 1.05;

const MAIN_VM_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_vm_snapshot as f32;

const CODE_DECOMMITTER_SORTER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_code_decommitter_sorter as f32;

const LOG_DEMUXER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_log_demuxer as f32;

const STORAGE_SORTER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_storage_sorter as f32;

const EVENTS_OR_L1_MESSAGES_SORTER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_events_or_l1_messages_sorter as f32;

const RAM_PERMUTATION_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_ram_permutation as f32;

pub(crate) const CODE_DECOMMITTER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_code_decommitter as f32;

pub(crate) const STORAGE_APPLICATION_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_storage_application as f32;

pub(crate) const KECCAK256_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_keccak256_circuit as f32;

pub(crate) const SHA256_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_sha256_circuit as f32;

pub(crate) const ECRECOVER_CYCLE_FRACTION: f32 =
    OVERESTIMATE_PERCENT / GEOMETRY_CONFIG.cycles_per_ecrecover_circuit as f32;

// "Rich addressing" opcodes are opcodes that can write their return value/read the input onto the stack
// and so take 1-2 RAM permutations more than an average opcode.
// In the worst case, a rich addressing may take 3 ram permutations
// (1 for reading the opcode, 1 for writing input value, 1 for writing output value).
pub(crate) const RICH_ADDRESSING_OPCODE_RAM_CYCLES: u32 = 3;
pub(crate) const RICH_ADDRESSING_OPCODE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + RICH_ADDRESSING_OPCODE_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION;

pub(crate) const AVERAGE_OPCODE_RAM_CYCLES: u32 = 1;
pub(crate) const AVERAGE_OPCODE_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + AVERAGE_OPCODE_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION;

pub(crate) const STORAGE_READ_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const STORAGE_READ_STORAGE_SORTER_CYCLES: u32 = 1;
// Here "base" fraction is a fraction that will be used unconditionally.
// Usage of `StorageApplication` is being tracked separately as it depends on whether slot was read before or not.
pub(crate) const STORAGE_READ_BASE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + STORAGE_READ_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION
    + STORAGE_READ_LOG_DEMUXER_CYCLES as f32 * LOG_DEMUXER_CYCLE_FRACTION
    + STORAGE_READ_STORAGE_SORTER_CYCLES as f32 * STORAGE_SORTER_CYCLE_FRACTION;

pub(crate) const EVENT_RAM_CYCLES: u32 = 1;
pub(crate) const EVENT_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const EVENT_EVENTS_SORTER_CYCLES: u32 = 2;
pub(crate) const EVENT_OR_L1_MESSAGE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + EVENT_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION
    + EVENT_LOG_DEMUXER_CYCLES as f32 * LOG_DEMUXER_CYCLE_FRACTION
    + EVENT_EVENTS_SORTER_CYCLES as f32 * EVENTS_OR_L1_MESSAGES_SORTER_CYCLE_FRACTION;

pub(crate) const STORAGE_WRITE_RAM_CYCLES: u32 = 1;
pub(crate) const STORAGE_WRITE_LOG_DEMUXER_CYCLES: u32 = 2;
pub(crate) const STORAGE_WRITE_STORAGE_SORTER_CYCLES: u32 = 2;
// Here "base" fraction is a fraction that will be used unconditionally.
// Usage of `StorageApplication` is being tracked separately as it depends on whether slot was written before or not.
pub(crate) const STORAGE_WRITE_BASE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + STORAGE_WRITE_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION
    + STORAGE_WRITE_LOG_DEMUXER_CYCLES as f32 * LOG_DEMUXER_CYCLE_FRACTION
    + STORAGE_WRITE_STORAGE_SORTER_CYCLES as f32 * STORAGE_SORTER_CYCLE_FRACTION;

pub(crate) const FAR_CALL_RAM_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_STORAGE_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES: u32 = 1;
pub(crate) const FAR_CALL_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + FAR_CALL_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION
    + FAR_CALL_STORAGE_SORTER_CYCLES as f32 * STORAGE_SORTER_CYCLE_FRACTION
    + FAR_CALL_CODE_DECOMMITTER_SORTER_CYCLES as f32 * CODE_DECOMMITTER_SORTER_CYCLE_FRACTION;

// 5 RAM permutations, because: 1 to read opcode + 2 reads + 2 writes.
// 2 reads and 2 writes are needed because unaligned access is implemented with
// aligned queries.
pub(crate) const UMA_WRITE_RAM_CYCLES: u32 = 5;
pub(crate) const UMA_WRITE_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + UMA_WRITE_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION;

// 3 RAM permutations, because: 1 to read opcode + 2 reads.
// 2 reads are needed because unaligned access is implemented with aligned queries.
pub(crate) const UMA_READ_RAM_CYCLES: u32 = 3;
pub(crate) const UMA_READ_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + UMA_READ_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION;

pub(crate) const PRECOMPILE_RAM_CYCLES: u32 = 1;
pub(crate) const PRECOMPILE_LOG_DEMUXER_CYCLES: u32 = 1;
pub(crate) const PRECOMPILE_CALL_COMMON_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + PRECOMPILE_RAM_CYCLES as f32 * RAM_PERMUTATION_CYCLE_FRACTION
    + PRECOMPILE_LOG_DEMUXER_CYCLES as f32 * LOG_DEMUXER_CYCLE_FRACTION;

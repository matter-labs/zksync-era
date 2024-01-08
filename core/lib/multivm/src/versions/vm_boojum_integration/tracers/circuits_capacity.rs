use zkevm_test_harness_1_4_0::{geometry_config::get_geometry_config, toolset::GeometryConfig};

const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();
const OVERESTIMATE_PERCENT: f32 = 1.05;

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
pub(crate) const RICH_ADDRESSING_OPCODE_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + 3.0 * RAM_PERMUTATION_CYCLE_FRACTION;

pub(crate) const AVERAGE_OPCODE_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + RAM_PERMUTATION_CYCLE_FRACTION;

// Here "base" fraction is a fraction that will be used unconditionally.
// Usage of `StorageApplication` is being tracked separately as it depends on whether slot was read before or not.
pub(crate) const STORAGE_READ_BASE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + RAM_PERMUTATION_CYCLE_FRACTION
    + LOG_DEMUXER_CYCLE_FRACTION
    + STORAGE_SORTER_CYCLE_FRACTION;

pub(crate) const EVENT_OR_L1_MESSAGE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + RAM_PERMUTATION_CYCLE_FRACTION
    + 2.0 * LOG_DEMUXER_CYCLE_FRACTION
    + 2.0 * EVENTS_OR_L1_MESSAGES_SORTER_CYCLE_FRACTION;

// Here "base" fraction is a fraction that will be used unconditionally.
// Usage of `StorageApplication` is being tracked separately as it depends on whether slot was written before or not.
pub(crate) const STORAGE_WRITE_BASE_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + RAM_PERMUTATION_CYCLE_FRACTION
    + 2.0 * LOG_DEMUXER_CYCLE_FRACTION
    + 2.0 * STORAGE_SORTER_CYCLE_FRACTION;

pub(crate) const FAR_CALL_FRACTION: f32 = MAIN_VM_CYCLE_FRACTION
    + RAM_PERMUTATION_CYCLE_FRACTION
    + STORAGE_SORTER_CYCLE_FRACTION
    + CODE_DECOMMITTER_SORTER_CYCLE_FRACTION;

// 5 RAM permutations, because: 1 to read opcode + 2 reads + 2 writes.
// 2 reads and 2 writes are needed because unaligned access is implemented with
// aligned queries.
pub(crate) const UMA_WRITE_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + 5.0 * RAM_PERMUTATION_CYCLE_FRACTION;

// 3 RAM permutations, because: 1 to read opcode + 2 reads.
// 2 reads are needed because unaligned access is implemented with aligned queries.
pub(crate) const UMA_READ_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + 3.0 * RAM_PERMUTATION_CYCLE_FRACTION;

pub(crate) const PRECOMPILE_CALL_COMMON_FRACTION: f32 =
    MAIN_VM_CYCLE_FRACTION + RAM_PERMUTATION_CYCLE_FRACTION + LOG_DEMUXER_CYCLE_FRACTION;

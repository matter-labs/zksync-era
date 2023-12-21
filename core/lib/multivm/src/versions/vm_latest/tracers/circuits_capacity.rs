use bigdecimal::BigDecimal;
use once_cell::sync::Lazy;
use zkevm_test_harness_1_4_0::{geometry_config::get_geometry_config, toolset::GeometryConfig};

const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();
const OVERESTIMATE_PERCENT: u32 = 5;

fn calculate_fraction(cycles_per_circuit: u32) -> BigDecimal {
    (BigDecimal::from(1) / BigDecimal::from(cycles_per_circuit)
        * (BigDecimal::from(1) + BigDecimal::from(OVERESTIMATE_PERCENT) / BigDecimal::from(100)))
    .with_prec(8)
}

static MAIN_VM_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_vm_snapshot));

static CODE_DECOMMITTER_SORTER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_code_decommitter_sorter));

static LOG_DEMUXER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_log_demuxer));

static STORAGE_SORTER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_storage_sorter));

static EVENTS_OR_L1_MESSAGES_SORTER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_events_or_l1_messages_sorter));

static RAM_PERMUTATION_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_ram_permutation));

pub(crate) static CODE_DECOMMITTER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_code_decommitter));

static STORAGE_APPLICATION_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_storage_application));

pub(crate) static KECCAK256_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_keccak256_circuit));

pub(crate) static SHA256_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_sha256_circuit));

pub(crate) static ECRECOVER_CYCLE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| calculate_fraction(GEOMETRY_CONFIG.cycles_per_ecrecover_circuit));

pub(crate) static RICH_ADDRESSING_OPCODE_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    MAIN_VM_CYCLE_FRACTION.clone() + BigDecimal::from(3) * RAM_PERMUTATION_CYCLE_FRACTION.clone()
});

pub(crate) static AVERAGE_OPCODE_FRACTION: Lazy<BigDecimal> =
    Lazy::new(|| MAIN_VM_CYCLE_FRACTION.clone() + RAM_PERMUTATION_CYCLE_FRACTION.clone());

pub(crate) static STORAGE_READ_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    MAIN_VM_CYCLE_FRACTION.clone()
        + RAM_PERMUTATION_CYCLE_FRACTION.clone()
        + LOG_DEMUXER_CYCLE_FRACTION.clone()
        + STORAGE_SORTER_CYCLE_FRACTION.clone()
        + STORAGE_APPLICATION_CYCLE_FRACTION.clone()
});

pub(crate) static EVENT_OR_L1_MESSAGE_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    BigDecimal::from(2) * MAIN_VM_CYCLE_FRACTION.clone()
        + RAM_PERMUTATION_CYCLE_FRACTION.clone()
        + BigDecimal::from(2) * LOG_DEMUXER_CYCLE_FRACTION.clone()
        + BigDecimal::from(2) * EVENTS_OR_L1_MESSAGES_SORTER_CYCLE_FRACTION.clone()
});

pub(crate) static HOT_STORAGE_WRITE_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    BigDecimal::from(2) * MAIN_VM_CYCLE_FRACTION.clone()
        + RAM_PERMUTATION_CYCLE_FRACTION.clone()
        + BigDecimal::from(2) * LOG_DEMUXER_CYCLE_FRACTION.clone()
        + BigDecimal::from(2) * STORAGE_SORTER_CYCLE_FRACTION.clone()
});

pub(crate) static COLD_STORAGE_WRITE_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    HOT_STORAGE_WRITE_FRACTION.clone()
        + BigDecimal::from(2) * STORAGE_APPLICATION_CYCLE_FRACTION.clone()
});

pub(crate) static FAR_CALL_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    BigDecimal::from(2) * MAIN_VM_CYCLE_FRACTION.clone()
        + RAM_PERMUTATION_CYCLE_FRACTION.clone()
        + STORAGE_SORTER_CYCLE_FRACTION.clone()
        + CODE_DECOMMITTER_SORTER_CYCLE_FRACTION.clone()
});

pub(crate) static UMA_WRITE_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    BigDecimal::from(2) * MAIN_VM_CYCLE_FRACTION.clone()
        + BigDecimal::from(5) * RAM_PERMUTATION_CYCLE_FRACTION.clone()
});

pub(crate) static UMA_READ_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    BigDecimal::from(2) * MAIN_VM_CYCLE_FRACTION.clone()
        + BigDecimal::from(3) * RAM_PERMUTATION_CYCLE_FRACTION.clone()
});

pub(crate) static PRECOMPILE_CALL_COMMON_FRACTION: Lazy<BigDecimal> = Lazy::new(|| {
    MAIN_VM_CYCLE_FRACTION.clone()
        + RAM_PERMUTATION_CYCLE_FRACTION.clone()
        + LOG_DEMUXER_CYCLE_FRACTION.clone()
});

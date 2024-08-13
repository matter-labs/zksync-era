// FIXME: move to multivm
/// Holds information about number of cycles used per circuit type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CircuitCycleStatistic {
    pub main_vm_cycles: u32,
    pub ram_permutation_cycles: u32,
    pub storage_application_cycles: u32,
    pub storage_sorter_cycles: u32,
    pub code_decommitter_cycles: u32,
    pub code_decommitter_sorter_cycles: u32,
    pub log_demuxer_cycles: u32,
    pub events_sorter_cycles: u32,
    pub keccak256_cycles: u32,
    pub ecrecover_cycles: u32,
    pub sha256_cycles: u32,
    pub secp256k1_verify_cycles: u32,
    pub transient_storage_checker_cycles: u32,
}

impl CircuitCycleStatistic {
    pub fn new() -> Self {
        Self::default()
    }
}

use std::ops::Add;

use serde::{Deserialize, Serialize};

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
    pub ecadd_cycles: u32,
    pub ecmul_cycles: u32,
    pub ecpairing_cycles: u32,
    pub modexp_cycles: u32,
    pub sha256_cycles: u32,
    pub secp256k1_verify_cycles: u32,
    pub transient_storage_checker_cycles: u32,
}

impl CircuitCycleStatistic {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Holds information about number of circuits used per circuit type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CircuitStatistic {
    pub main_vm: f32,
    pub ram_permutation: f32,
    pub storage_application: f32,
    pub storage_sorter: f32,
    pub code_decommitter: f32,
    pub code_decommitter_sorter: f32,
    pub log_demuxer: f32,
    pub events_sorter: f32,
    pub keccak256: f32,
    pub ecrecover: f32,
    pub ecadd: f32,
    pub ecmul: f32,
    pub ecpairing: f32,
    pub modexp: f32,
    pub sha256: f32,
    #[serde(default)]
    pub secp256k1_verify: f32,
    #[serde(default)]
    pub transient_storage_checker: f32,
}

impl CircuitStatistic {
    /// Rounds up numbers and adds them.
    pub fn total(&self) -> usize {
        self.main_vm.ceil() as usize
            + self.ram_permutation.ceil() as usize
            + self.storage_application.ceil() as usize
            + self.storage_sorter.ceil() as usize
            + self.code_decommitter.ceil() as usize
            + self.code_decommitter_sorter.ceil() as usize
            + self.log_demuxer.ceil() as usize
            + self.events_sorter.ceil() as usize
            + self.keccak256.ceil() as usize
            + self.ecrecover.ceil() as usize
            + self.ecadd.ceil() as usize
            + self.ecmul.ceil() as usize
            + self.ecpairing.ceil() as usize
            + self.modexp.ceil() as usize
            + self.sha256.ceil() as usize
            + self.secp256k1_verify.ceil() as usize
            + self.transient_storage_checker.ceil() as usize
    }

    /// Adds numbers.
    pub fn total_f32(&self) -> f32 {
        self.main_vm
            + self.ram_permutation
            + self.storage_application
            + self.storage_sorter
            + self.code_decommitter
            + self.code_decommitter_sorter
            + self.log_demuxer
            + self.events_sorter
            + self.keccak256
            + self.ecrecover
            + self.ecadd
            + self.ecmul
            + self.ecpairing
            + self.modexp
            + self.sha256
            + self.secp256k1_verify
            + self.transient_storage_checker
    }
}

impl Add for CircuitStatistic {
    type Output = CircuitStatistic;

    fn add(self, other: CircuitStatistic) -> CircuitStatistic {
        CircuitStatistic {
            main_vm: self.main_vm + other.main_vm,
            ram_permutation: self.ram_permutation + other.ram_permutation,
            storage_application: self.storage_application + other.storage_application,
            storage_sorter: self.storage_sorter + other.storage_sorter,
            code_decommitter: self.code_decommitter + other.code_decommitter,
            code_decommitter_sorter: self.code_decommitter_sorter + other.code_decommitter_sorter,
            log_demuxer: self.log_demuxer + other.log_demuxer,
            events_sorter: self.events_sorter + other.events_sorter,
            keccak256: self.keccak256 + other.keccak256,
            ecrecover: self.ecrecover + other.ecrecover,
            ecadd: self.ecadd + other.ecadd,
            ecmul: self.ecmul + other.ecmul,
            ecpairing: self.ecpairing + other.ecpairing,
            modexp: self.modexp + other.modexp,
            sha256: self.sha256 + other.sha256,
            secp256k1_verify: self.secp256k1_verify + other.secp256k1_verify,
            transient_storage_checker: self.transient_storage_checker
                + other.transient_storage_checker,
        }
    }
}

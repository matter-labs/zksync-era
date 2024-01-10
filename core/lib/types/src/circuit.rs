use std::ops::Add;

use serde::{Deserialize, Serialize};
use zkevm_test_harness_1_4_0::{geometry_config::get_geometry_config, toolset::GeometryConfig};

pub const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();

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
    pub sha256: f32,
}

impl CircuitStatistic {
    pub fn from_cycles(cycles: CircuitCycleStatistic, geometry: GeometryConfig) -> Self {
        Self {
            main_vm: cycles.main_vm_cycles as f32 / geometry.cycles_per_vm_snapshot as f32,
            ram_permutation: cycles.ram_permutation_cycles as f32
                / geometry.cycles_per_ram_permutation as f32,
            storage_application: cycles.storage_application_cycles as f32
                / geometry.cycles_per_storage_application as f32,
            storage_sorter: cycles.storage_sorter_cycles as f32
                / geometry.cycles_per_storage_sorter as f32,
            code_decommitter: cycles.code_decommitter_cycles as f32
                / geometry.cycles_per_code_decommitter as f32,
            code_decommitter_sorter: cycles.code_decommitter_sorter_cycles as f32
                / geometry.cycles_code_decommitter_sorter as f32,
            log_demuxer: cycles.log_demuxer_cycles as f32 / geometry.cycles_per_log_demuxer as f32,
            events_sorter: cycles.events_sorter_cycles as f32
                / geometry.cycles_per_events_or_l1_messages_sorter as f32,
            keccak256: cycles.keccak256_cycles as f32
                / geometry.cycles_per_keccak256_circuit as f32,
            ecrecover: cycles.ecrecover_cycles as f32
                / geometry.cycles_per_ecrecover_circuit as f32,
            sha256: cycles.sha256_cycles as f32 / geometry.cycles_per_sha256_circuit as f32,
        }
    }

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
            + self.sha256.ceil() as usize
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
            + self.sha256
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
            sha256: self.sha256 + other.sha256,
        }
    }
}

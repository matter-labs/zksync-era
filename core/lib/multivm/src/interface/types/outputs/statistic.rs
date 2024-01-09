use zkevm_test_harness_1_4_0::toolset::GeometryConfig;

/// Statistics of the tx execution.
#[derive(Debug, Default, Clone)]
pub struct VmExecutionStatistics {
    /// Number of contracts used by the VM during the tx execution.
    pub contracts_used: usize,
    /// Cycles used by the VM during the tx execution.
    pub cycles_used: u32,
    /// Gas used by the VM during the tx execution.
    pub gas_used: u32,
    /// Computational gas used by the VM during the tx execution.
    pub computational_gas_used: u32,
    /// Number of log queries produced by the VM during the tx execution.
    pub total_log_queries: usize,
    pub pubdata_published: u32,
    pub estimated_circuits_used: f32,
    pub circuit_statistic: Option<CircuitStatistic>,
}

/// Oracle metrics of the VM.
pub struct VmMemoryMetrics {
    pub event_sink_inner: usize,
    pub event_sink_history: usize,
    pub memory_inner: usize,
    pub memory_history: usize,
    pub decommittment_processor_inner: usize,
    pub decommittment_processor_history: usize,
    pub storage_inner: usize,
    pub storage_history: usize,
}

impl VmMemoryMetrics {
    pub fn full_size(&self) -> usize {
        [
            self.event_sink_inner,
            self.event_sink_history,
            self.memory_inner,
            self.memory_history,
            self.decommittment_processor_inner,
            self.decommittment_processor_history,
            self.storage_inner,
            self.storage_history,
        ]
        .iter()
        .sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CircuitStatistic {
    pub main_vm: u32,
    pub ram_permutation: u32,
    pub storage_application_by_writes: u32,
    pub storage_application_by_reads: u32,
    pub storage_sorter: u32,
    pub code_decommitter: u32,
    pub code_decommitter_sorter: u32,
    pub log_demuxer: u32,
    pub events_sorter: u32,
    pub keccak256: u32,
    pub ecrecover: u32,
    pub sha256: u32,
}

impl CircuitStatistic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total(&self, geometry: &GeometryConfig) -> f32 {
        self.main_vm as f32 / geometry.cycles_per_vm_snapshot as f32
            + self.ram_permutation as f32 / geometry.cycles_per_ram_permutation as f32
            + self.storage_application_by_writes as f32
                / geometry.cycles_per_storage_application as f32
            + self.storage_application_by_reads as f32
                / geometry.cycles_per_storage_application as f32
            + self.storage_sorter as f32 / geometry.cycles_per_storage_sorter as f32
            + self.code_decommitter as f32 / geometry.cycles_per_code_decommitter as f32
            + self.code_decommitter_sorter as f32 / geometry.cycles_code_decommitter_sorter as f32
            + self.log_demuxer as f32 / geometry.cycles_per_log_demuxer as f32
            + self.events_sorter as f32 / geometry.cycles_per_events_or_l1_messages_sorter as f32
            + self.keccak256 as f32 / geometry.cycles_per_keccak256_circuit as f32
            + self.ecrecover as f32 / geometry.cycles_per_ecrecover_circuit as f32
            + self.sha256 as f32 / geometry.cycles_per_sha256_circuit as f32
    }
}

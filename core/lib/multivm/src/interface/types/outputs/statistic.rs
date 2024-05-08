use zksync_types::circuit::CircuitStatistic;

/// Statistics of the tx execution.
#[derive(Debug, Default, Clone)]
pub struct VmExecutionStatistics {
    /// Number of contracts used by the VM during the tx execution.
    pub contracts_used: usize,
    /// Cycles used by the VM during the tx execution.
    pub cycles_used: u32,
    /// Gas used by the VM during the tx execution.
    pub gas_used: u64,
    /// Gas remaining after the tx execution.
    pub gas_remaining: u32,
    /// Computational gas used by the VM during the tx execution.
    pub computational_gas_used: u32,
    /// Number of log queries produced by the VM during the tx execution.
    pub total_log_queries: usize,
    pub pubdata_published: u32,
    pub circuit_statistic: CircuitStatistic,
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

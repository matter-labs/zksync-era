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
}

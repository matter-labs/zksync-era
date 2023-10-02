use zk_evm::aux_structures::Timestamp;
use zksync_state::WriteStorage;

use zksync_types::U256;

use crate::old_vm::history_recorder::HistoryMode;
use crate::tracers::DefaultExecutionTracer;
use crate::vm::Vm;
use vm_latest::VmExecutionStatistics;

/// Module responsible for observing the VM behavior, i.e. calculating the statistics of the VM runs
/// or reporting the VM memory usage.

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    /// Get statistics about TX execution.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_statistics(
        &self,
        timestamp_initial: Timestamp,
        cycles_initial: u32,
        tracer: &DefaultExecutionTracer<S, H>,
        gas_remaining_before: u32,
        gas_remaining_after: u32,
        spent_pubdata_counter_before: u32,
        total_log_queries_count: usize,
    ) -> VmExecutionStatistics {
        let computational_gas_used = self.calculate_computational_gas_used(
            tracer,
            gas_remaining_before,
            spent_pubdata_counter_before,
        );
        VmExecutionStatistics {
            contracts_used: self
                .state
                .decommittment_processor
                .get_decommitted_bytecodes_after_timestamp(timestamp_initial),
            cycles_used: self.state.local_state.monotonic_cycle_counter - cycles_initial,
            gas_used: gas_remaining_before - gas_remaining_after,
            computational_gas_used,
            total_log_queries: total_log_queries_count,
        }
    }

    /// Returns the hashes the bytecodes that have been decommitted by the decomittment processor.
    pub(crate) fn get_used_contracts(&self) -> Vec<U256> {
        self.state
            .decommittment_processor
            .decommitted_code_hashes
            .inner()
            .keys()
            .cloned()
            .collect()
    }
}

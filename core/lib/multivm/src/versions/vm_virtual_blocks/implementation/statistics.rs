use zk_evm_1_3_3::aux_structures::Timestamp;
use zksync_state::WriteStorage;

use crate::interface::{VmExecutionStatistics, VmMemoryMetrics};
use crate::HistoryMode;
use zksync_types::U256;

use crate::vm_virtual_blocks::tracers::DefaultExecutionTracer;
use crate::vm_virtual_blocks::vm::Vm;

/// Module responsible for observing the VM behavior, i.e. calculating the statistics of the VM runs
/// or reporting the VM memory usage.

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    /// Get statistics about TX execution.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_statistics(
        &self,
        timestamp_initial: Timestamp,
        cycles_initial: u32,
        tracer: &DefaultExecutionTracer<S, H::VmVirtualBlocksMode>,
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
            // This field will be populated by the RefundTracer
            pubdata_published: 0,
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

    /// Returns the info about all oracles' sizes.
    pub(crate) fn record_vm_memory_metrics_inner(&self) -> VmMemoryMetrics {
        VmMemoryMetrics {
            event_sink_inner: self.state.event_sink.get_size(),
            event_sink_history: self.state.event_sink.get_history_size(),
            memory_inner: self.state.memory.get_size(),
            memory_history: self.state.memory.get_history_size(),
            decommittment_processor_inner: self.state.decommittment_processor.get_size(),
            decommittment_processor_history: self.state.decommittment_processor.get_history_size(),
            storage_inner: self.state.storage.get_size(),
            storage_history: self.state.storage.get_history_size(),
        }
    }
}

//! Module responsible for observing the VM behavior, i.e. calculating the statistics of the VM runs
//! or reporting the VM memory usage.

use std::time::Duration;

use vm::{HistoryMode, VmExecutionResult, VmInstance};
use zksync_state::StorageViewMetrics;
use zksync_types::{
    event::{extract_long_l2_to_l1_messages, extract_published_bytecodes},
    fee::TransactionExecutionMetrics,
    storage_writes_deduplicator::StorageWritesDeduplicator,
};
use zksync_utils::bytecode::bytecode_len_in_bytes;

pub(super) fn report_storage_view_metrics(
    tx_id: &str,
    oracles_sizes: usize,
    vm_execution_took: Duration,
    metrics: StorageViewMetrics,
) {
    metrics::histogram!(
        "runtime_context.memory.storage_view_cache_size",
        metrics.cache_size as f64
    );
    metrics::histogram!(
        "runtime_context.memory",
        (oracles_sizes + metrics.cache_size) as f64
    );

    let total_storage_invocations =
        metrics.get_value_storage_invocations + metrics.set_value_storage_invocations;
    let total_time_spent_in_storage =
        metrics.time_spent_on_get_value + metrics.time_spent_on_set_value;

    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        metrics.storage_invocations_missed as f64,
        "interaction" => "missed"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        metrics.get_value_storage_invocations as f64,
        "interaction" => "get_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        metrics.set_value_storage_invocations as f64,
        "interaction" => "set_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        total_storage_invocations as f64,
        "interaction" => "total"
    );

    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        metrics.time_spent_on_storage_missed,
        "interaction" => "missed"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        metrics.time_spent_on_get_value,
        "interaction" => "get_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        metrics.time_spent_on_set_value,
        "interaction" => "set_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        total_time_spent_in_storage,
        "interaction" => "total"
    );

    if total_storage_invocations > 0 {
        metrics::histogram!(
            "runtime_context.storage_interaction.duration_per_unit",
            total_time_spent_in_storage.div_f64(total_storage_invocations as f64),
            "interaction" => "total"
        );
    }
    if metrics.storage_invocations_missed > 0 {
        let duration_per_unit = metrics
            .time_spent_on_storage_missed
            .div_f64(metrics.storage_invocations_missed as f64);
        metrics::histogram!(
            "runtime_context.storage_interaction.duration_per_unit",
            duration_per_unit,
            "interaction" => "missed"
        );
    }

    metrics::histogram!(
        "runtime_context.storage_interaction.ratio",
        total_time_spent_in_storage.as_secs_f64() / vm_execution_took.as_secs_f64(),
    );

    const STORAGE_INVOCATIONS_DEBUG_THRESHOLD: usize = 1_000;

    if total_storage_invocations > STORAGE_INVOCATIONS_DEBUG_THRESHOLD {
        vlog::info!(
            "Tx {tx_id} resulted in {total_storage_invocations} storage_invocations, {} new_storage_invocations, \
             {} get_value_storage_invocations, {} set_value_storage_invocations, \
             vm execution took {vm_execution_took:?}, storage interaction took {total_time_spent_in_storage:?} \
             (missed: {:?} get: {:?} set: {:?})",
            metrics.storage_invocations_missed,
            metrics.get_value_storage_invocations,
            metrics.set_value_storage_invocations,
            metrics.time_spent_on_storage_missed,
            metrics.time_spent_on_get_value,
            metrics.time_spent_on_set_value,
        );
    }
}

pub(super) fn collect_tx_execution_metrics(
    contracts_deployed: u16,
    result: &VmExecutionResult,
) -> TransactionExecutionMetrics {
    let event_topics = result
        .events
        .iter()
        .map(|event| event.indexed_topics.len() as u16)
        .sum();

    let l2_l1_long_messages = extract_long_l2_to_l1_messages(&result.events)
        .iter()
        .map(|event| event.len())
        .sum();

    let published_bytecode_bytes = extract_published_bytecodes(&result.events)
        .iter()
        .map(|bytecode_hash| bytecode_len_in_bytes(*bytecode_hash))
        .sum();

    let writes_metrics =
        StorageWritesDeduplicator::apply_on_empty_state(&result.storage_log_queries);

    TransactionExecutionMetrics {
        initial_storage_writes: writes_metrics.initial_storage_writes,
        repeated_storage_writes: writes_metrics.repeated_storage_writes,
        gas_used: result.gas_used as usize,
        event_topics,
        published_bytecode_bytes,
        l2_l1_long_messages,
        l2_l1_logs: result.l2_to_l1_logs.len(),
        contracts_used: result.contracts_used,
        contracts_deployed,
        vm_events: result.events.len(),
        storage_logs: result.storage_log_queries.len(),
        total_log_queries: result.total_log_queries,
        cycles_used: result.cycles_used,
        computational_gas_used: result.computational_gas_used,
    }
}

/// Returns the sum of all oracles' sizes.
pub(super) fn record_vm_memory_metrics<H: HistoryMode>(vm: &VmInstance<'_, H>) -> usize {
    let event_sink_inner = vm.state.event_sink.get_size();
    let event_sink_history = vm.state.event_sink.get_history_size();
    let memory_inner = vm.state.memory.get_size();
    let memory_history = vm.state.memory.get_history_size();
    let decommittment_processor_inner = vm.state.decommittment_processor.get_size();
    let decommittment_processor_history = vm.state.decommittment_processor.get_history_size();
    let storage_inner = vm.state.storage.get_size();
    let storage_history = vm.state.storage.get_history_size();

    metrics::histogram!("runtime_context.memory.event_sink_size", event_sink_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.event_sink_size", event_sink_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.memory_size", memory_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.memory_size", memory_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.decommitter_size", decommittment_processor_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.decommitter_size", decommittment_processor_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.storage_size", storage_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.storage_size", storage_history as f64, "type" => "history");

    [
        event_sink_inner,
        event_sink_history,
        memory_inner,
        memory_history,
        decommittment_processor_inner,
        decommittment_processor_history,
        storage_inner,
        storage_history,
    ]
    .iter()
    .sum::<usize>()
}

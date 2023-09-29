use std::time::Duration;
use vm::{VmExecutionResultAndLogs, VmMemoryMetrics};
use zksync_state::StorageViewMetrics;
use zksync_types::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use zksync_types::fee::TransactionExecutionMetrics;
use zksync_types::storage_writes_deduplicator::StorageWritesDeduplicator;
use zksync_utils::bytecode::bytecode_len_in_bytes;

pub fn report_vm_memory_metrics(
    tx_id: &str,
    memory_metrics: &VmMemoryMetrics,
    vm_execution_took: Duration,
    storage_metrics: StorageViewMetrics,
) {
    metrics::histogram!("runtime_context.memory.event_sink_size", memory_metrics.event_sink_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.event_sink_size", memory_metrics.event_sink_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.memory_size", memory_metrics.memory_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.memory_size", memory_metrics.memory_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.decommitter_size", memory_metrics.decommittment_processor_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.decommitter_size", memory_metrics.decommittment_processor_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.storage_size", memory_metrics.storage_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.storage_size", memory_metrics.storage_history as f64, "type" => "history");

    metrics::histogram!(
        "runtime_context.memory.storage_view_cache_size",
        storage_metrics.cache_size as f64
    );
    metrics::histogram!(
        "runtime_context.memory",
        (memory_metrics.full_size() + storage_metrics.cache_size) as f64
    );

    let total_storage_invocations = storage_metrics.get_value_storage_invocations
        + storage_metrics.set_value_storage_invocations;
    let total_time_spent_in_storage =
        storage_metrics.time_spent_on_get_value + storage_metrics.time_spent_on_set_value;

    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        storage_metrics.storage_invocations_missed as f64,
        "interaction" => "missed"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        storage_metrics.get_value_storage_invocations as f64,
        "interaction" => "get_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        storage_metrics.set_value_storage_invocations as f64,
        "interaction" => "set_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.amount",
        total_storage_invocations as f64,
        "interaction" => "total"
    );

    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        storage_metrics.time_spent_on_storage_missed,
        "interaction" => "missed"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        storage_metrics.time_spent_on_get_value,
        "interaction" => "get_value"
    );
    metrics::histogram!(
        "runtime_context.storage_interaction.duration",
        storage_metrics.time_spent_on_set_value,
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
    if storage_metrics.storage_invocations_missed > 0 {
        let duration_per_unit = storage_metrics
            .time_spent_on_storage_missed
            .div_f64(storage_metrics.storage_invocations_missed as f64);
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
        tracing::info!(
            "Tx {tx_id} resulted in {total_storage_invocations} storage_invocations, {} new_storage_invocations, \
             {} get_value_storage_invocations, {} set_value_storage_invocations, \
             vm execution took {vm_execution_took:?}, storage interaction took {total_time_spent_in_storage:?} \
             (missed: {:?} get: {:?} set: {:?})",
            storage_metrics.storage_invocations_missed,
            storage_metrics.get_value_storage_invocations,
            storage_metrics.set_value_storage_invocations,
            storage_metrics.time_spent_on_storage_missed,
            storage_metrics.time_spent_on_get_value,
            storage_metrics.time_spent_on_set_value,
        );
    }
}

pub(super) fn collect_tx_execution_metrics(
    contracts_deployed: u16,
    result: &VmExecutionResultAndLogs,
) -> TransactionExecutionMetrics {
    let writes_metrics = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    let event_topics = result
        .logs
        .events
        .iter()
        .map(|event| event.indexed_topics.len() as u16)
        .sum();
    let l2_l1_long_messages = extract_long_l2_to_l1_messages(&result.logs.events)
        .iter()
        .map(|event| event.len())
        .sum();
    let published_bytecode_bytes = extract_published_bytecodes(&result.logs.events)
        .iter()
        .map(|bytecode_hash| bytecode_len_in_bytes(*bytecode_hash))
        .sum();

    TransactionExecutionMetrics {
        initial_storage_writes: writes_metrics.initial_storage_writes,
        repeated_storage_writes: writes_metrics.repeated_storage_writes,
        gas_used: result.statistics.gas_used as usize,
        event_topics,
        published_bytecode_bytes,
        l2_l1_long_messages,
        l2_l1_logs: result.logs.total_l2_to_l1_logs_count(),
        contracts_used: result.statistics.contracts_used,
        contracts_deployed,
        vm_events: result.logs.events.len(),
        storage_logs: result.logs.storage_logs.len(),
        total_log_queries: result.statistics.total_log_queries,
        cycles_used: result.statistics.cycles_used,
        computational_gas_used: result.statistics.computational_gas_used,
    }
}

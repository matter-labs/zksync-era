use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

use std::time::Duration;

use multivm::interface::{VmExecutionResultAndLogs, VmMemoryMetrics};
use zksync_state::StorageViewMetrics;
use zksync_types::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use zksync_types::fee::TransactionExecutionMetrics;
use zksync_types::storage_writes_deduplicator::StorageWritesDeduplicator;
use zksync_utils::bytecode::bytecode_len_in_bytes;

use crate::metrics::InteractionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
enum SizeType {
    Inner,
    History,
}

const MEMORY_SIZE_BUCKETS: Buckets = Buckets::values(&[
    1_000.0,
    10_000.0,
    100_000.0,
    500_000.0,
    1_000_000.0,
    5_000_000.0,
    10_000_000.0,
    50_000_000.0,
    100_000_000.0,
    500_000_000.0,
    1_000_000_000.0,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "runtime_context_memory")]
struct RuntimeContextMemoryMetrics {
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    event_sink_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    memory_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    decommitter_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    storage_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    storage_view_cache_size: Histogram<usize>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    full: Histogram<usize>,
}

#[vise::register]
static MEMORY_METRICS: vise::Global<RuntimeContextMemoryMetrics> = vise::Global::new();

const INTERACTION_AMOUNT_BUCKETS: Buckets = Buckets::exponential(10.0..=10_000_000.0, 10.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "runtime_context_storage_interaction")]
struct RuntimeContextStorageMetrics {
    #[metrics(buckets = INTERACTION_AMOUNT_BUCKETS)]
    amount: Family<InteractionType, Histogram<usize>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    duration: Family<InteractionType, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    duration_per_unit: Family<InteractionType, Histogram<Duration>>,
    #[metrics(buckets = Buckets::ZERO_TO_ONE)]
    ratio: Histogram<f64>,
}

#[vise::register]
static STORAGE_METRICS: vise::Global<RuntimeContextStorageMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum SandboxStage {
    VmConcurrencyLimiterAcquire,
    Initialization,
    ValidateInSandbox,
    Validation,
    Execution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(in crate::api_server) enum SubmitTxStage {
    #[metrics(name = "1_validate")]
    Validate,
    #[metrics(name = "2_dry_run")]
    DryRun,
    #[metrics(name = "3_verify_execute")]
    VerifyExecute,
    #[metrics(name = "4_tx_proxy")]
    TxProxy,
    #[metrics(name = "4_db_insert")]
    DbInsert,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3")]
pub(in crate::api_server) struct SandboxMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub(super) sandbox: Family<SandboxStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::linear(0.0..=2_000.0, 200.0))]
    pub(super) sandbox_execution_permits: Histogram<usize>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub submit_tx: Family<SubmitTxStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::linear(0.0..=30.0, 3.0))]
    pub estimate_gas_binary_search_iterations: Histogram<usize>,
}

#[vise::register]
pub(in crate::api_server) static SANDBOX_METRICS: vise::Global<SandboxMetrics> =
    vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_execution")]
pub(super) struct ExecutionMetrics {
    pub tokens_amount: Gauge<usize>,
    pub trusted_address_slots_amount: Gauge<usize>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub get_validation_params: Histogram<Duration>,
}

#[vise::register]
pub(super) static EXECUTION_METRICS: vise::Global<ExecutionMetrics> = vise::Global::new();

pub(super) fn report_vm_memory_metrics(
    tx_id: &str,
    memory_metrics: &VmMemoryMetrics,
    vm_execution_took: Duration,
    storage_metrics: StorageViewMetrics,
) {
    MEMORY_METRICS.event_sink_size[&SizeType::Inner].observe(memory_metrics.event_sink_inner);
    MEMORY_METRICS.event_sink_size[&SizeType::History].observe(memory_metrics.event_sink_history);
    MEMORY_METRICS.memory_size[&SizeType::Inner].observe(memory_metrics.memory_inner);
    MEMORY_METRICS.memory_size[&SizeType::History].observe(memory_metrics.memory_history);
    MEMORY_METRICS.decommitter_size[&SizeType::Inner]
        .observe(memory_metrics.decommittment_processor_inner);
    MEMORY_METRICS.decommitter_size[&SizeType::History]
        .observe(memory_metrics.decommittment_processor_history);
    MEMORY_METRICS.storage_size[&SizeType::Inner].observe(memory_metrics.storage_inner);
    MEMORY_METRICS.storage_size[&SizeType::History].observe(memory_metrics.storage_history);

    MEMORY_METRICS
        .storage_view_cache_size
        .observe(storage_metrics.cache_size);
    MEMORY_METRICS
        .full
        .observe(memory_metrics.full_size() + storage_metrics.cache_size);

    let total_storage_invocations = storage_metrics.get_value_storage_invocations
        + storage_metrics.set_value_storage_invocations;
    let total_time_spent_in_storage =
        storage_metrics.time_spent_on_get_value + storage_metrics.time_spent_on_set_value;

    STORAGE_METRICS.amount[&InteractionType::Missed]
        .observe(storage_metrics.storage_invocations_missed);
    STORAGE_METRICS.amount[&InteractionType::GetValue]
        .observe(storage_metrics.get_value_storage_invocations);
    STORAGE_METRICS.amount[&InteractionType::SetValue]
        .observe(storage_metrics.set_value_storage_invocations);
    STORAGE_METRICS.amount[&InteractionType::Total].observe(total_storage_invocations);

    STORAGE_METRICS.duration[&InteractionType::Missed]
        .observe(storage_metrics.time_spent_on_storage_missed);
    STORAGE_METRICS.duration[&InteractionType::GetValue]
        .observe(storage_metrics.time_spent_on_get_value);
    STORAGE_METRICS.duration[&InteractionType::SetValue]
        .observe(storage_metrics.time_spent_on_set_value);
    STORAGE_METRICS.duration[&InteractionType::Total].observe(total_time_spent_in_storage);

    if total_storage_invocations > 0 {
        STORAGE_METRICS.duration_per_unit[&InteractionType::Total]
            .observe(total_time_spent_in_storage.div_f64(total_storage_invocations as f64));
    }
    if storage_metrics.storage_invocations_missed > 0 {
        let duration_per_unit = storage_metrics
            .time_spent_on_storage_missed
            .div_f64(storage_metrics.storage_invocations_missed as f64);
        STORAGE_METRICS.duration_per_unit[&InteractionType::Missed].observe(duration_per_unit);
    }

    STORAGE_METRICS
        .ratio
        .observe(total_time_spent_in_storage.as_secs_f64() / vm_execution_took.as_secs_f64());

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
        total_updated_values_size: writes_metrics.total_updated_values_size,
        pubdata_published: result.statistics.pubdata_published,
    }
}

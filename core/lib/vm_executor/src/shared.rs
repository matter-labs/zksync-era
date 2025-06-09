//! Functionality shared among different types of executors.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, MetricsFamily};
use zksync_multivm::interface::storage::StorageViewStats;

/// Marker for sealed traits. Intentionally not exported from the crate.
pub trait Sealed {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "interaction", rename_all = "snake_case")]
pub(crate) enum InteractionType {
    Missed,
    GetValue,
    SetValue,
    Total,
}

const INTERACTION_AMOUNT_BUCKETS: Buckets = Buckets::exponential(10.0..=10_000_000.0, 10.0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, EncodeLabelSet)]
struct GlobalStorageLabels {
    /// Was the VM execution interrupted?
    interrupted: bool,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "runtime_context_storage_interaction")]
pub(crate) struct RuntimeContextStorageMetrics {
    #[metrics(buckets = INTERACTION_AMOUNT_BUCKETS)]
    amount: Family<InteractionType, Histogram<usize>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    duration: Family<InteractionType, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    duration_per_unit: Family<InteractionType, Histogram<Duration>>,
    #[metrics(buckets = Buckets::ZERO_TO_ONE)]
    ratio: Histogram<f64>,
}

impl RuntimeContextStorageMetrics {
    pub(crate) fn observe(
        op: &str,
        interrupted: bool,
        total_vm_latency: Duration,
        storage_metrics: &StorageViewStats,
    ) {
        const STORAGE_INVOCATIONS_DEBUG_THRESHOLD: usize = 1_000;
        const TOTAL_VM_LATENCY_THRESHOLD: Duration = Duration::from_secs(5);

        let metrics = &STORAGE_METRICS[&GlobalStorageLabels { interrupted }];
        let total_storage_invocations = storage_metrics.get_value_storage_invocations
            + storage_metrics.set_value_storage_invocations;
        let total_time_spent_in_storage =
            storage_metrics.time_spent_on_get_value + storage_metrics.time_spent_on_set_value;

        metrics.amount[&InteractionType::Missed]
            .observe(storage_metrics.storage_invocations_missed);
        metrics.amount[&InteractionType::GetValue]
            .observe(storage_metrics.get_value_storage_invocations);
        metrics.amount[&InteractionType::SetValue]
            .observe(storage_metrics.set_value_storage_invocations);
        metrics.amount[&InteractionType::Total].observe(total_storage_invocations);

        metrics.duration[&InteractionType::Missed]
            .observe(storage_metrics.time_spent_on_storage_missed);
        metrics.duration[&InteractionType::GetValue]
            .observe(storage_metrics.time_spent_on_get_value);
        metrics.duration[&InteractionType::SetValue]
            .observe(storage_metrics.time_spent_on_set_value);
        metrics.duration[&InteractionType::Total].observe(total_time_spent_in_storage);

        if total_storage_invocations > 0 {
            metrics.duration_per_unit[&InteractionType::Total]
                .observe(total_time_spent_in_storage.div_f64(total_storage_invocations as f64));
        }
        if storage_metrics.storage_invocations_missed > 0 {
            let duration_per_unit = storage_metrics
                .time_spent_on_storage_missed
                .div_f64(storage_metrics.storage_invocations_missed as f64);
            metrics.duration_per_unit[&InteractionType::Missed].observe(duration_per_unit);
        }

        metrics
            .ratio
            .observe(total_time_spent_in_storage.as_secs_f64() / total_vm_latency.as_secs_f64());

        if total_storage_invocations > STORAGE_INVOCATIONS_DEBUG_THRESHOLD
            || total_vm_latency > TOTAL_VM_LATENCY_THRESHOLD
            || interrupted
        {
            tracing::info!(
                interrupted,
                "{op} resulted in {total_storage_invocations} storage_invocations, {} new_storage_invocations, \
                 {} get_value_storage_invocations, {} set_value_storage_invocations, \
                 vm execution took {total_vm_latency:?}, storage interaction took {total_time_spent_in_storage:?} \
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
}

#[vise::register]
static STORAGE_METRICS: MetricsFamily<GlobalStorageLabels, RuntimeContextStorageMetrics> =
    MetricsFamily::new();

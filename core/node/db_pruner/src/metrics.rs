use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};
use zksync_dal::pruning_dal::HardPruningStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "prune_type", rename_all = "snake_case")]
pub(super) enum PruneType {
    NoOp,
    Soft,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
enum PrunedEntityType {
    L1Batch,
    L2Block,
    StorageLog,
    Event,
    L2ToL1Log,
    CallTrace,
}

const ENTITY_COUNT_BUCKETS: Buckets = Buckets::values(&[
    1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "db_pruner")]
pub(super) struct DbPrunerMetrics {
    /// Total latency of pruning chunk of L1 batches.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub pruning_chunk_duration: Family<PruneType, Histogram<Duration>>,
    /// Number of not-pruned L1 batches.
    pub not_pruned_l1_batches_count: Gauge<u64>,
    /// Number of entities deleted during a single hard pruning iteration, grouped by entity type.
    #[metrics(buckets = ENTITY_COUNT_BUCKETS)]
    deleted_entities: Family<PrunedEntityType, Histogram<u64>>,
}

impl DbPrunerMetrics {
    pub fn observe_hard_pruning(&self, stats: HardPruningStats) {
        let HardPruningStats {
            deleted_l1_batches,
            deleted_l2_blocks,
            deleted_storage_logs,
            deleted_events,
            deleted_call_traces,
            deleted_l2_to_l1_logs,
        } = stats;
        tracing::info!(
            "Performed pruning of database, deleted {deleted_l1_batches} L1 batches, {deleted_l2_blocks} L2 blocks, \
             {deleted_storage_logs} storage logs, \
             {deleted_events} events, {deleted_call_traces} call traces, {deleted_l2_to_l1_logs} L2-to-L1 logs"
        );

        self.deleted_entities[&PrunedEntityType::L1Batch].observe(deleted_l1_batches);
        self.deleted_entities[&PrunedEntityType::L2Block].observe(deleted_l2_blocks);
        self.deleted_entities[&PrunedEntityType::StorageLog].observe(deleted_storage_logs);
        self.deleted_entities[&PrunedEntityType::Event].observe(deleted_events);
        self.deleted_entities[&PrunedEntityType::L2ToL1Log].observe(deleted_l2_to_l1_logs);
        self.deleted_entities[&PrunedEntityType::CallTrace].observe(deleted_call_traces);
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<DbPrunerMetrics> = vise::Global::new();

//! Metrics for `MetadataCalculator`.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};
use zksync_config::configs::database::MerkleTreeMode;
use zksync_shared_metrics::tree::{ConfigLabels, ModeLabel};

use super::MetadataCalculatorConfig;

impl MetadataCalculatorConfig {
    pub(crate) fn as_labels(&self) -> ConfigLabels {
        ConfigLabels {
            mode: match self.mode {
                MerkleTreeMode::Full => ModeLabel::Full,
                MerkleTreeMode::Lightweight => ModeLabel::Lightweight,
            },
            delay_interval: self.delay_interval.into(),
            max_l1_batches_per_iter: self.max_l1_batches_per_iter,
            multi_get_chunk_size: self.multi_get_chunk_size,
            block_cache_capacity: self.block_cache_capacity,
            memtable_capacity: self.memtable_capacity,
            stalled_writes_timeout: self.stalled_writes_timeout.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum RecoveryStage {
    LoadChunkStarts,
    Finalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ChunkRecoveryStage {
    AcquireConnection,
    LoadEntries,
    LockTree,
    ExtendTree,
}

/// Metrics for Merkle tree recovery driven by the metadata calculator.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_metadata_calculator_recovery")]
pub(super) struct MetadataCalculatorRecoveryMetrics {
    /// Number of chunks recovered.
    pub recovered_chunk_count: Gauge<u64>,
    /// Latency of a tree recovery stage (not related to the recovery of a particular chunk;
    /// those metrics are tracked in the `chunk_latency` histogram).
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub latency: Family<RecoveryStage, Histogram<Duration>>,
    /// Latency of a chunk recovery stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub chunk_latency: Family<ChunkRecoveryStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static RECOVERY_METRICS: vise::Global<MetadataCalculatorRecoveryMetrics> =
    vise::Global::new();

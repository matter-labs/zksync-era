use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum CommitmentStage {
    PrepareInput,
    Calculate,
    SaveResults,
}

const BATCH_COUNT_BUCKETS: Buckets = Buckets::linear(1.0..=16.0, 1.0);

/// Metrics for the commitment generator.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_commitment_generator")]
pub(super) struct CommitmentGeneratorMetrics {
    /// Latency of generating commitment for a single L1 batch per stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub generate_commitment_latency_stage: Family<CommitmentStage, Histogram<Duration>>,
    /// Latency of generating bootloader content commitment for a single L1 batch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub bootloader_content_commitment_latency: Histogram<Duration>,
    /// Latency of generating events queue commitment for a single L1 batch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub events_queue_commitment_latency: Histogram<Duration>,

    /// Latency of processing a continuous chunk of L1 batches during a single step of the generator.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub step_latency: Histogram<Duration>,
    /// Number of L1 batches processed during a single step.
    #[metrics(buckets = BATCH_COUNT_BUCKETS)]
    pub step_batch_count: Histogram<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<CommitmentGeneratorMetrics> = vise::Global::new();

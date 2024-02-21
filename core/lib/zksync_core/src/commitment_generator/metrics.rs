use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum CommitmentStage {
    PrepareInput,
    Calculate,
    SaveResults,
}

/// Metrics for the commitment generator.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_commitment_generator")]
pub(super) struct CommitmentGeneratorMetrics {
    /// Latency of generating commitment per stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub generate_commitment_latency_stage: Family<CommitmentStage, Histogram<Duration>>,
    /// Latency of generating bootloader content commitment.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub bootloader_content_commitment_latency: Histogram<Duration>,
    /// Latency of generating events queue commitment.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub events_queue_commitment_latency: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<CommitmentGeneratorMetrics> = vise::Global::new();

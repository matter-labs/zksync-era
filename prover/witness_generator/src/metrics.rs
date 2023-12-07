use std::time::Duration;

use vise::{Buckets, Counter, Family, Gauge, Histogram, LabeledFamily, Metrics};
use zksync_prover_fri_utils::metrics::StageLabel;

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_witness_generator")]
pub(crate) struct WitnessGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prepare_job_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub witness_generation_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_save_time: Family<StageLabel, Histogram<Duration>>,

    pub sampled_blocks: Counter,
    pub skipped_blocks: Counter,
}

#[vise::register]
pub(crate) static WITNESS_GENERATOR_METRICS: vise::Global<WitnessGeneratorMetrics> =
    vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover")]
pub(crate) struct ServerMetrics {
    #[metrics(labels = ["stage"])]
    pub init_latency: LabeledFamily<StageLabel, Gauge<Duration>>,
}

#[vise::register]
pub(crate) static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();

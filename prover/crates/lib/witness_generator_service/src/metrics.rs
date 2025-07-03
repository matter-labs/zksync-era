use std::time::Duration;

use vise::{Buckets, Family, Histogram, Metrics};
use zksync_prover_fri_utils::metrics::StageLabel;

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_witness_generator")]
pub(crate) struct WitnessGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prepare_job_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::exponential(60.0..=61440.0, 2.0))]
    pub witness_generation_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_save_time: Family<StageLabel, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static WITNESS_GENERATOR_METRICS: vise::Global<WitnessGeneratorMetrics> =
    vise::Global::new();

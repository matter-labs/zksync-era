use std::time::Duration;
use vise::{Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri.proof_fri_compressor")]
pub(crate) struct ProofFriCompressorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub compression_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_save_time: Histogram<Duration>,
}

#[vise::register]
pub(crate) static PROOF_FRI_COMPRESSOR_METRICS: vise::Global<ProofFriCompressorMetrics> =
    vise::Global::new();

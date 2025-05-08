use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_proof_fri_compressor_service")]
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

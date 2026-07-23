use std::net::SocketAddr;
use std::time::Duration;

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub enum ProofStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(rename_all = "snake_case", label = "kind")]
pub enum ProofType {
    Fri,
    Snark,
}

impl std::fmt::Display for ProofType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofType::Fri => f.write_str("FRI"),
            ProofType::Snark => f.write_str("SNARK"),
        }
    }
}

/// Labels attached to proof generation metrics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub struct ProofLabels {
    /// L2 chain id of the chain the batch belongs to, as reported by the job
    /// server it was fetched from. Distinguishes chains when one prover serves
    /// several; 0 when the server predates chain-id reporting.
    pub chain_id: u64,
    pub batch_number: u32,
    pub proof_type: ProofType,
    pub status: ProofStatus,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover")]
pub struct ProverMetrics {
    /// Time taken by the GPU prover to generate a proof, in seconds.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub proof_duration: Family<ProofLabels, Histogram<Duration>>,

    /// Total number of proof generation attempts.
    pub proof_count: Family<ProofLabels, Counter>,

    /// Number of jobs currently in flight, labeled by phase. A FRI job is
    /// counted under `fri` from fetch until its FRI proof is submitted, and a
    /// SNARK job is counted under `snark` from when it enters the SNARK
    /// pipeline (either a server-fetched SNARK job, or the local follow-up in
    /// `fri-snark` mode) until its SNARK proof is submitted.
    pub pending_jobs: Family<ProofType, Gauge>,
}

#[vise::register]
pub static METRICS: vise::Global<ProverMetrics> = vise::Global::new();

/// Starts the Prometheus metrics scrape endpoint on the given port.
///
/// Spawns a background thread with its own single-threaded tokio runtime.
/// Returns an error if the thread cannot be spawned. Panics inside the thread
/// if the HTTP server fails to bind or exits unexpectedly.
pub fn start_metrics_server(port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    std::thread::Builder::new()
        .name("metrics-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime for metrics server");
            rt.block_on(async move {
                vise_exporter::MetricsExporter::default()
                    .start(addr)
                    .await
                    .expect("metrics server error");
            });
        })
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("failed to spawn metrics server thread: {err}"))
}

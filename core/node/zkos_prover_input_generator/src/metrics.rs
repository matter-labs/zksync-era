use std::time::Duration;

use vise::{Counter, Family, Gauge, Histogram, Metrics};
use zksync_state_keeper::metrics::{TxExecutionResult, TxExecutionType};

/// General-purpose state keeper metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "provers_state")]
pub struct ProverStateMetrics {
    pub latest_submitted_fri_proof: Gauge<usize>,
    pub latest_submitted_snark_proof: Gauge<usize>,
    pub fri_queue: Gauge<usize>,
    pub snark_queue: Gauge<usize>,
}

#[vise::register]
pub static PROVER_STATE_METRICS: vise::Global<ProverStateMetrics> = vise::Global::new();

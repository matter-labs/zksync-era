use std::time::Duration;

use vise::{Gauge, LabeledFamily, Metrics};
use zksync_prover_fri_utils::metrics::StageLabel;

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover")]
pub struct ServerMetrics {
    #[metrics(labels = ["stage"])]
    pub init_latency: LabeledFamily<StageLabel, Gauge<Duration>>,
}

#[vise::register]
pub static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();

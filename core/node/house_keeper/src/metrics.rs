use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "fri_prover")]
pub(crate) struct FriProverMetrics {
    pub oldest_unpicked_batch: Gauge<u64>,
    pub oldest_not_generated_batch: Gauge<u64>,
}

#[vise::register]
pub(crate) static FRI_PROVER_METRICS: vise::Global<FriProverMetrics> = vise::Global::new();

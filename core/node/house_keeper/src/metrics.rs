use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "fri_prover")]
pub(crate) struct FriProverMetrics {
    pub oldest_unpicked_batch: Gauge<u64>,
    pub oldest_not_generated_batch: Gauge<u64>,
    /// Number of batches that are ready to be proven by airbender provers.
    pub airbender_batches_ready_for_proving: Gauge<u64>,
    /// Number of batches whose FRI proof is ready and are waiting to be wrapped into a SNARK proof.
    pub airbender_batches_ready_for_snark: Gauge<u64>,
}

#[vise::register]
pub(crate) static FRI_PROVER_METRICS: vise::Global<FriProverMetrics> = vise::Global::new();

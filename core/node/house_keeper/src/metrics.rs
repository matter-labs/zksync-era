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
    /// Oldest batch still waiting for its proof input blobs (VM run data / witness inputs).
    /// Such a batch is invisible to airbender provers, so `airbender_batches_ready_for_proving`
    /// stays at 0 while work silently queues behind a stalled input producer. 0 = none pending.
    pub airbender_oldest_input_pending_batch: Gauge<u64>,
    /// Seconds the oldest input-pending batch has been waiting for its input blobs.
    /// The alerting signal for input-producer lag: provers idle + this growing = the gap is
    /// upstream of the proving pipeline. 0 = none pending.
    pub airbender_oldest_input_pending_batch_age_secs: Gauge<u64>,
}

#[vise::register]
pub(crate) static FRI_PROVER_METRICS: vise::Global<FriProverMetrics> = vise::Global::new();

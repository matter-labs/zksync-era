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

#[derive(Debug, Metrics)]
#[metrics(prefix = "house_keeper")]
pub(crate) struct TwoFactorApprovalMetrics {
    /// Seconds elapsed since the L1 commit confirmation of the oldest committed batch that is
    /// still missing enough 2FA approvals to be executed via `EraMultisigValidator`. Reset to 0
    /// once the batch has enough approvals or there is nothing pending execution.
    /// Not emitted for chains that don't use `EraMultisigValidator`.
    pub committed_batch_2fa_approval_pending_seconds: Gauge<u64>,
}

#[vise::register]
pub(crate) static TWO_FACTOR_APPROVAL_METRICS: vise::Global<TwoFactorApprovalMetrics> =
    vise::Global::new();

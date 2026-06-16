use std::time::Duration;

use vise::{Buckets, Counter, EncodeLabelValue, Histogram, LabeledFamily, Metrics, Unit};

/// Buckets for proof roundtrip times, ranging from 1 minute to 4 hours.
const PROOF_ROUNDTRIP_BUCKETS: Buckets = Buckets::values(&[
    60.0, 120.0, 300.0, 600.0, 900.0, 1200.0, 1800.0, 3600.0, 7200.0, 14400.0,
]);

/// Proving stage a metric refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum ProofStage {
    /// The FRI proof produced directly by an Airbender prover.
    Fri,
    /// The SNARK proof wrapping the FRI proof, ready for L1.
    Snark,
}

/// Kind of error surfaced by the request processor, used to categorize failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum ProcessorErrorKind {
    /// Catch-all internal error (`AirbenderProcessorError::GeneralError`).
    General,
    /// Hard object-store failure surfaced to the client (`AirbenderProcessorError::ObjectStore`).
    ObjectStore,
    /// Database failure (`AirbenderProcessorError::Dal`).
    Dal,
    /// Soft retry: inputs/proof not yet available on GCS, the batch lock is rolled back and retried.
    ObjectStoreKeyNotFound,
    /// A job-handout endpoint exhausted `max_attempts` looking for a batch with available data.
    AttemptsExhausted,
}

#[derive(Debug, Metrics)]
pub(crate) struct AirbenderProofDataHandlerMetrics {
    /// FRI proof roundtrip time, from batch seal to FRI proof submission.
    #[metrics(buckets = PROOF_ROUNDTRIP_BUCKETS, unit = Unit::Seconds)]
    pub airbender_proof_roundtrip_time: Histogram<Duration>,
    /// SNARK proof roundtrip time, from batch seal to SNARK proof submission.
    #[metrics(buckets = PROOF_ROUNDTRIP_BUCKETS, unit = Unit::Seconds)]
    pub airbender_snark_roundtrip_time: Histogram<Duration>,
    /// Proofs received by the handler, split by stage and protocol version.
    #[metrics(labels = ["stage", "protocol_version"])]
    pub airbender_proofs_received: LabeledFamily<(ProofStage, String), Counter, 2>,
    /// Proving jobs handed out to provers, split by stage and protocol version.
    #[metrics(labels = ["stage", "protocol_version"])]
    pub airbender_jobs_picked: LabeledFamily<(ProofStage, String), Counter, 2>,
    /// Proof failures reported by provers, split by stage.
    #[metrics(labels = ["stage"])]
    pub airbender_proof_failures: LabeledFamily<ProofStage, Counter>,
    /// Errors surfaced by the request processor, split by kind.
    #[metrics(labels = ["kind"])]
    pub airbender_processor_errors: LabeledFamily<ProcessorErrorKind, Counter>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<AirbenderProofDataHandlerMetrics> = vise::Global::new();

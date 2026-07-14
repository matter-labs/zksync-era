use airbender_host::Proof;
use clap::ValueEnum;
use eravm_prover_host::SnarkWrapperProof;
use zksync_prover_metrics::ProofType;

/// Jobs received from the job worker. Which variant is expected is implied
/// by which pipelines the worker was built with; a mismatch is a job-worker bug.
pub enum WorkerJob {
    Fri {
        batch_number: u32,
        input_words: Vec<u32>,
    },
    Snark {
        batch_number: u32,
        proof: Box<Proof>,
    },
}

impl WorkerJob {
    pub fn batch_number(&self) -> u32 {
        match self {
            WorkerJob::Fri { batch_number, .. } | WorkerJob::Snark { batch_number, .. } => {
                *batch_number
            }
        }
    }

    pub fn kind(&self) -> ProofType {
        match self {
            WorkerJob::Fri { .. } => ProofType::Fri,
            WorkerJob::Snark { .. } => ProofType::Snark,
        }
    }
}

/// Operating mode for the prover server.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ProverMode {
    /// Poll for FRI inputs, run the FRI prover, submit FRI proofs.
    FriOnly,
    /// Poll for FRI inputs, run FRI + SNARK back-to-back, submit both.
    FriSnark,
    /// Poll for ready FRI proofs, run the SNARK wrapper, submit SNARK proofs.
    SnarkOnly,
}

/// Successful proving outcome emitted by the prover worker. Failures travel
/// as the `Err` arm of [`ProverResult`] over the same channel. Every fetched
/// job produces at least one [`ProverResult`] — failures included — so the
/// job worker can account for it exactly once. In `fri-snark` mode a single
/// fetched job emits two results (FRI then SNARK); each settles its own
/// `pending_jobs` bucket (FRI then SNARK) as it lands.
pub enum ProofOutcome {
    Fri {
        batch_number: u32,
        proof: Box<Proof>,
        /// Guest cycles executed by the FRI prover's RISC-V run for this batch,
        /// reported to the server alongside the proof.
        cycles_used: u64,
    },
    Snark {
        batch_number: u32,
        proof: Box<SnarkWrapperProof>,
    },
}

impl ProofOutcome {
    pub fn kind(&self) -> ProofType {
        match self {
            ProofOutcome::Fri { .. } => ProofType::Fri,
            ProofOutcome::Snark { .. } => ProofType::Snark,
        }
    }
}

/// Failure detail carried in the `Err` arm of [`ProverResult`]. Holds the
/// kind and batch number so the job worker can route the failure log without
/// inspecting the success type.
pub struct FailedProof {
    pub batch_number: u32,
    pub kind: ProofType,
    /// Full anyhow error chain (`{err:#}`) captured at the point of failure.
    pub reason: String,
}

impl FailedProof {
    pub fn new(batch_number: u32, kind: ProofType, err: anyhow::Error) -> Self {
        Self {
            batch_number,
            kind,
            reason: format!("{err:#}"),
        }
    }
}

/// Message type sent by the prover worker: either a successful
/// [`ProofOutcome`] or a [`FailedProof`].
pub type ProverResult = Result<ProofOutcome, FailedProof>;

/// Mirrors `SubmitAirbenderProofRequest` from zksync-era. Carries either a
/// `proof` (success) or an `error` (the prover could not produce the proof),
/// which releases the batch for retry — bounded by the server's configured
/// attempts limit — without waiting for the proving timeout to elapse. Exactly
/// one of `proof`/`error` is set; the unset field is omitted from the JSON so
/// the server's `#[serde(default)]` fields default to `None`.
/// The `proof` bytes are hex-encoded in JSON, matching the `#[serde_as(as = "Option<Hex>")]` annotation.
#[serde_with::serde_as]
#[derive(serde::Serialize)]
pub struct SubmitFriProofRequest<'a> {
    pub l1_batch_number: u32,
    pub prover_id: String,
    #[serde_as(as = "Option<serde_with::hex::Hex>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<&'a [u8]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Guest cycles the prover actually executed for this batch. Sent with a
    /// successful proof and omitted on failure submissions; the server's
    /// `#[serde(default)]` defaults the missing field to `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycles_used: Option<u64>,
}

/// SNARK submission payload. Like [`SubmitFriProofRequest`], carries either a
/// `snark_proof` or an `error`. The VK is resolved once at startup and is not
/// included here; the receiver is expected to know it out of band.
/// `snark_proof` is carried as a nested JSON object (the wrapper PLONK proof),
/// matching `SubmitAirbenderSnarkProofRequest` in zksync-era.
#[derive(serde::Serialize)]
pub struct SubmitSnarkProofRequest {
    pub l1_batch_number: u32,
    pub prover_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snark_proof: Option<Box<SnarkWrapperProof>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fri_success_serializes_hex_proof_and_omits_error() {
        let json = serde_json::to_value(SubmitFriProofRequest {
            l1_batch_number: 42,
            prover_id: "prover-1".to_string(),
            proof: Some(&[10, 11, 12, 13, 14]),
            error: None,
            cycles_used: Some(123_456_789),
        })
        .unwrap();
        assert_eq!(json["l1_batch_number"], 42);
        assert_eq!(json["prover_id"], "prover-1");
        assert_eq!(json["proof"], "0a0b0c0d0e");
        // A successful submission reports the batch's measured guest cycles.
        assert_eq!(json["cycles_used"], 123_456_789);
        // The server defaults `error` to `None`, so a success submission omits it.
        assert!(json.get("error").is_none());
    }

    #[test]
    fn fri_failure_serializes_error_and_omits_proof() {
        let json = serde_json::to_value(SubmitFriProofRequest {
            l1_batch_number: 42,
            prover_id: "prover-1".to_string(),
            proof: None,
            error: Some("prover ran out of memory".to_string()),
            cycles_used: None,
        })
        .unwrap();
        assert_eq!(json["error"], "prover ran out of memory");
        assert!(json.get("proof").is_none());
        // No proof means no cycle count; the field is omitted on failure.
        assert!(json.get("cycles_used").is_none());
    }

    #[test]
    fn snark_failure_serializes_error_and_omits_proof() {
        let json = serde_json::to_value(SubmitSnarkProofRequest {
            l1_batch_number: 42,
            prover_id: "prover-1".to_string(),
            snark_proof: None,
            error: Some("snark wrapper panicked".to_string()),
        })
        .unwrap();
        assert_eq!(json["error"], "snark wrapper panicked");
        assert!(json.get("snark_proof").is_none());
    }
}

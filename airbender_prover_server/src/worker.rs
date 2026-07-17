use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, SendError, SyncSender};
use std::time::{Duration, Instant};

use airbender_host::Proof;
use anyhow::{Context, Result};
use eravm_prover_host::{FriProver, FriVerifier, RawFriProof, SnarkPipeline};
use tracing::info;
use zksync_prover_metrics::{ProofLabels, ProofStatus, ProofType, METRICS};

use crate::types::{FailedProof, ProofOutcome, ProverResult, WorkerJob};

pub struct ProverWorker {
    fri: Option<Box<dyn FriProver>>,
    fri_verifier: Option<FriVerifier>,
    snark: Option<SnarkPipeline>,
    job_rx: Receiver<WorkerJob>,
    result_tx: SyncSender<ProverResult>,
}

/// Fluent builder for [`ProverWorker`]. Pipelines are added one at a time; the
/// combination is validated by [`Self::build`] against the supported modes.
#[derive(Default)]
pub struct ProverWorkerBuilder {
    fri: Option<Box<dyn FriProver>>,
    fri_verifier: Option<FriVerifier>,
    snark: Option<SnarkPipeline>,
}

impl ProverWorkerBuilder {
    pub fn with_fri(mut self, fri: Box<dyn FriProver>) -> Self {
        self.fri = Some(fri);
        self
    }

    pub fn with_fri_verifier(mut self, verifier: FriVerifier) -> Self {
        self.fri_verifier = Some(verifier);
        self
    }

    pub fn with_snark(mut self, snark: SnarkPipeline) -> Self {
        self.snark = Some(snark);
        self
    }

    /// Validates that at least one pipeline is configured and returns a
    /// ready-to-run [`ProverWorker`]. Valid combinations: `fri-only`,
    /// `fri-snark` (both pipelines), `snark-only` (SNARK pipeline with an
    /// attached FRI verifier).
    pub fn build(
        self,
        job_rx: Receiver<WorkerJob>,
        result_tx: SyncSender<ProverResult>,
    ) -> Result<ProverWorker> {
        let has_fri = self.fri.is_some();

        if !has_fri && self.snark.is_none() {
            anyhow::bail!("ProverWorker builder: must set at least one of `fri` or `snark`");
        }
        if has_fri && self.fri_verifier.is_some() {
            anyhow::bail!(
                "ProverWorker builder: `fri_verifier` is redundant when `fri` is set — \
                 the FRI pipeline verifies the proofs it produces"
            );
        }
        if self.fri_verifier.is_some() && self.snark.is_none() {
            anyhow::bail!("ProverWorker builder: `fri_verifier` requires `snark`");
        }
        Ok(ProverWorker {
            fri: self.fri,
            fri_verifier: self.fri_verifier,
            snark: self.snark,
            job_rx,
            result_tx,
        })
    }
}

impl ProverWorker {
    pub fn builder() -> ProverWorkerBuilder {
        ProverWorkerBuilder::default()
    }

    /// Receives jobs and streams outcomes back. Exits when either channel closes.
    pub fn run(mut self) {
        while let Ok(job) = self.job_rx.recv() {
            if self.process(job).is_err() {
                return;
            }
        }
    }

    fn process(&mut self, job: WorkerJob) -> Result<(), SendError<ProverResult>> {
        let result = match job {
            WorkerJob::Fri {
                origin,
                batch_number,
                input_words,
            } => match self.fri.as_mut() {
                Some(fri) => {
                    info!(chain_id = origin.chain_id, kind = %ProofType::Fri, "Started proving batch {batch_number}");
                    let started = Instant::now();
                    let result = catch_prover_panic("FRI prover", || {
                        fri.prove_input(batch_number as u64, &input_words)
                    });
                    record_proof_metrics(
                        origin.chain_id,
                        batch_number,
                        ProofType::Fri,
                        status_of(&result),
                        started.elapsed(),
                    );
                    result
                        .map(|out| ProofOutcome::Fri {
                            origin,
                            batch_number,
                            proof: Box::new(out.proof),
                            cycles_used: out.cycles,
                        })
                        .map_err(|err| FailedProof::new(origin, batch_number, ProofType::Fri, err))
                }
                // `snark-only` workers (including every CUDA-free build) carry
                // no FRI prover and the job worker never enqueues FRI jobs;
                // fail loudly if one ever arrives.
                None => Err(FailedProof::new(
                    origin,
                    batch_number,
                    ProofType::Fri,
                    anyhow::anyhow!("received a FRI job but this worker has no FRI prover"),
                )),
            },
            WorkerJob::Snark {
                origin,
                batch_number,
                proof,
            } => match self.prepare_snark_input(batch_number, *proof) {
                Ok(raw_proof) => {
                    info!(chain_id = origin.chain_id, kind = %ProofType::Snark, "Started proving batch {batch_number}");
                    let started = Instant::now();
                    let snark = self.snark.as_mut().unwrap();
                    let result = catch_prover_panic("SNARK prover", || snark.prove(raw_proof));
                    record_proof_metrics(
                        origin.chain_id,
                        batch_number,
                        ProofType::Snark,
                        status_of(&result),
                        started.elapsed(),
                    );
                    result
                        .map(|proof| ProofOutcome::Snark {
                            origin,
                            batch_number,
                            proof: Box::new(proof),
                        })
                        .map_err(|err| {
                            FailedProof::new(origin, batch_number, ProofType::Snark, err)
                        })
                }
                Err(err) => Err(FailedProof::new(
                    origin,
                    batch_number,
                    ProofType::Snark,
                    err,
                )),
            },
        };
        self.result_tx.send(result)
    }

    /// Strips the proof envelope down to a raw FRI proof for the SNARK
    /// wrapper. When the worker carries a standalone FRI verifier (the
    /// `snark-only` mode), the envelope is also verified against the cached
    /// VK before unwrapping — `fri-snark` mode skips this because the proof
    /// has already been verified by [`FriPipeline::prove_input`].
    fn prepare_snark_input(&self, batch_number: u32, proof: Proof) -> Result<RawFriProof> {
        if let Some(verifier) = self.fri_verifier.as_ref() {
            verifier
                .verify_envelope(batch_number as u64, &proof)
                .with_context(|| {
                    format!("rejecting unverified FRI proof for batch {batch_number}")
                })?;
            info!(batch_number, "Verified incoming FRI proof");
        }
        match proof {
            Proof::Real(real) => Ok(real.into_inner()),
            Proof::Dev(_) => {
                anyhow::bail!("received development FRI proof; refusing to wrap into SNARK")
            }
        }
    }
}

/// Runs the prover closure, converting a panic into an `Err` so a single
/// poison input — e.g. a RISC-V guest that exhausts its memory and panics
/// mid-proof — is reported as a failed proof instead of unwinding the prover
/// thread and tearing down the whole process. The panic is still printed to
/// stderr by the default panic hook before we recover it here.
///
/// Note: a genuine host-side allocation failure aborts the process and cannot
/// be caught; this recovers panics (the common case for in-emulator OOM and
/// assertion failures), not aborts. The pipeline is reused for the next job on
/// the assumption that the failing job's memory is released on unwind.
fn catch_prover_panic<T>(label: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => Err(anyhow::anyhow!(
            "{label} panicked: {}",
            panic_payload_message(payload.as_ref())
        )),
    }
}

/// Extracts a human-readable message from a caught panic payload, which is
/// typically a `&str` or `String` for `panic!`-style aborts.
fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn status_of<T>(result: &Result<T>) -> ProofStatus {
    if result.is_ok() {
        ProofStatus::Success
    } else {
        ProofStatus::Failure
    }
}

fn record_proof_metrics(
    chain_id: u64,
    batch_number: u32,
    proof_type: ProofType,
    status: ProofStatus,
    elapsed: Duration,
) {
    let labels = ProofLabels {
        chain_id,
        batch_number,
        proof_type,
        status,
    };
    METRICS.proof_duration[&labels].observe(elapsed);
    METRICS.proof_count[&labels].inc();
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tracing::{debug, error, info, warn};
use zksync_prover_metrics::{ProofType, METRICS};

use crate::client::JobServerClient;
use crate::types::{ProofOutcome, ProverMode, ProverResult, WorkerJob};

/// Orchestrates the network side of the prover: fetches jobs from the
/// [`JobServerClient`], forwards them to the prover thread, and submits
/// completed proofs through the client. Uses a one-slot pending buffer so a
/// server-fetched FRI job can be pre-fetched while the prover is busy. In
/// `fri-snark` mode, a finished FRI produces a local SNARK follow-up that
/// lives in its own slot so it never clobbers the prefetched FRI.
pub struct JobWorker {
    mode: ProverMode,
    client: JobServerClient,
    // `Option` so shutdown can drop the sender (telling the prover no more jobs
    // are coming) while keeping the rest of the worker alive to drain results.
    job_tx: Option<SyncSender<WorkerJob>>,
    result_rx: Receiver<ProverResult>,
    poll_interval: Duration,
    shutdown: Arc<AtomicBool>,
    pending_job: Option<WorkerJob>,
    snark_followup: Option<WorkerJob>,
}

impl JobWorker {
    pub fn new(
        client: JobServerClient,
        job_tx: SyncSender<WorkerJob>,
        result_rx: Receiver<ProverResult>,
        shutdown: Arc<AtomicBool>,
        mode: ProverMode,
        poll_interval: Duration,
    ) -> Self {
        Self {
            mode,
            client,
            job_tx: Some(job_tx),
            result_rx,
            poll_interval,
            shutdown,
            pending_job: None,
            snark_followup: None,
        }
    }

    pub fn run(mut self) {
        loop {
            // Observe shutdown before touching the channels: stop fetching and
            // dispatching new work, then drain whatever the prover is still
            // computing so its result is submitted rather than discarded.
            if self.shutdown.load(Ordering::Relaxed) {
                self.drain_pending_results();
                return;
            }

            let mut did_work = false;

            // Drain the local SNARK follow-up before the prefetched FRI:
            // it represents work whose FRI half is already settled, and
            // keeping the order stable means a fresh prefetched FRI can
            // sit safely in `pending_job` until the prover is free again.
            if let Some(job) = self.snark_followup.take() {
                match self.sender().try_send(job) {
                    Ok(()) => did_work = true,
                    Err(TrySendError::Full(job)) => self.snark_followup = Some(job),
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
            if let Some(job) = self.pending_job.take() {
                match self.sender().try_send(job) {
                    Ok(()) => did_work = true,
                    Err(TrySendError::Full(job)) => self.pending_job = Some(job),
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }

            match self.result_rx.try_recv() {
                Ok(outcome) => {
                    if let Err(err) = self.handle_prover_result(outcome) {
                        error!(?err, "Failed to handle prover outcome");
                    }
                    did_work = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }

            if self.pending_job.is_none() {
                match self.fetch_job() {
                    Ok(Some(job)) => {
                        info!(batch_number = job.batch_number(), kind = %job.kind(), "Received job");
                        METRICS.pending_jobs[&job.kind()].inc_by(1);
                        self.pending_job = Some(job);
                        did_work = true;
                    }
                    Ok(None) => debug!("No jobs available, waiting..."),
                    Err(err) => warn!(?err, "Failed to fetch job, retrying after poll interval"),
                }
            }

            if !did_work {
                std::thread::sleep(self.poll_interval);
            }
        }
    }

    /// The job sender, present until [`Self::drain_pending_results`] drops it.
    /// The main loop only dispatches before shutdown, so it is always `Some`
    /// here.
    fn sender(&self) -> &SyncSender<WorkerJob> {
        self.job_tx
            .as_ref()
            .expect("job_tx is dropped only during shutdown drain")
    }

    /// Graceful shutdown: stop dispatching buffered work, drop the job sender
    /// so the prover sees no more jobs are coming, and block submitting every
    /// result the prover still produces for in-flight (and already-queued)
    /// jobs. Without this, returning from `run` immediately would drop
    /// `result_rx`, so the in-flight proof — potentially minutes of GPU work —
    /// would be computed and then silently discarded instead of submitted.
    fn drain_pending_results(&mut self) {
        info!("Shutting down: draining in-flight prover results before exit...");
        // Dropping the sender disconnects the channel once the prover drains
        // it: the prover finishes its current job, picks up any already-queued
        // job, then exits, which ends the blocking loop below.
        self.job_tx = None;
        while let Ok(outcome) = self.result_rx.recv() {
            if let Err(err) = self.handle_prover_result(outcome) {
                error!(?err, "Failed to handle prover outcome during shutdown");
            }
        }
        info!("All in-flight results submitted; prover stopped");
    }

    fn fetch_job(&self) -> Result<Option<WorkerJob>> {
        match self.mode {
            ProverMode::FriOnly | ProverMode::FriSnark => self.client.fetch_fri_job(),
            ProverMode::SnarkOnly => self.client.fetch_snark_job(),
        }
    }

    fn handle_prover_result(&mut self, outcome: ProverResult) -> Result<()> {
        let kind = match &outcome {
            Ok(o) => o.kind(),
            Err(f) => f.kind,
        };
        METRICS.pending_jobs[&kind].dec_by(1);
        let batch_number = match outcome {
            Ok(ProofOutcome::Fri {
                batch_number,
                proof,
                cycles_used,
            }) => {
                self.client
                    .submit_fri(batch_number, proof.as_ref(), cycles_used)?;
                // In `fri-snark` mode, the SNARK job needs the FRI proof, so we can set the new pending job immediately instead of waiting for the next fetch cycle. The in-memory `Proof` is fed directly to the SNARK pipeline without an extra encode/decode round trip.
                if self.mode == ProverMode::FriSnark {
                    // FRI jobs are processed serially, so the previous SNARK
                    // follow-up must have been drained before this one lands.
                    METRICS.pending_jobs[&ProofType::Snark].inc_by(1);
                    self.snark_followup = Some(WorkerJob::Snark {
                        batch_number,
                        proof,
                    });
                }
                batch_number
            }
            Ok(ProofOutcome::Snark {
                batch_number,
                proof,
            }) => {
                self.client.submit_snark(batch_number, proof)?;
                batch_number
            }
            Err(failure) => {
                // Report the failure to the server so it can release the batch
                // for retry (bounded by the server's attempts limit) without
                // waiting for the proving timeout to elapse.
                warn!(
                    batch_number = failure.batch_number,
                    kind = %failure.kind,
                    reason = %failure.reason,
                    "Prover job failed; reporting to server",
                );
                match failure.kind {
                    ProofType::Fri => self
                        .client
                        .submit_fri_error(failure.batch_number, &failure.reason)?,
                    ProofType::Snark => self
                        .client
                        .submit_snark_error(failure.batch_number, &failure.reason)?,
                }
                info!(
                    batch_number = failure.batch_number,
                    kind = %failure.kind,
                    "Reported failed proof to server",
                );
                return Ok(());
            }
        };
        info!(batch_number, kind = %kind, "Successfully submitted proof");
        Ok(())
    }
}

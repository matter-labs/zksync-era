use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
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
/// [`JobServerClient`]s, forwards them to the prover thread, and submits
/// completed proofs through the originating client. Uses a one-slot pending
/// buffer so a server-fetched FRI job can be pre-fetched while the prover is
/// busy. In `fri-snark` mode, a finished FRI produces a local SNARK follow-up
/// that lives in its own slot so it never clobbers the prefetched FRI.
///
/// With several clients configured (one job server per chain), fetches
/// round-robin across them: each fetch scans the chains starting right after
/// the one last served, taking the first job found. Chains with no work cost
/// one 204 poll and are skipped, so when every chain has a backlog each gets
/// an equal share of this prover, and idle chains donate their share — a
/// chain producing more batches never crowds the others out.
pub struct JobWorker {
    mode: ProverMode,
    /// One client per configured job server; a job origin's `server` indexes this.
    clients: Vec<JobServerClient>,
    /// Rotation cursor: the server the next fetch scan starts from.
    next_server: usize,
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
        clients: Vec<JobServerClient>,
        job_tx: SyncSender<WorkerJob>,
        result_rx: Receiver<ProverResult>,
        shutdown: Arc<AtomicBool>,
        mode: ProverMode,
        poll_interval: Duration,
    ) -> Self {
        assert!(!clients.is_empty(), "at least one job server is required");
        // Start the rotation at a random server so a fleet of provers booted
        // together doesn't scan the server list in lockstep, all competing for
        // the first chain's batches before spilling onto the next.
        // `RandomState` is seeded per process — cheap std-only entropy.
        let next_server = RandomState::new().build_hasher().finish() as usize % clients.len();
        Self {
            mode,
            clients,
            next_server,
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
                    Some(job) => {
                        info!(
                            chain_id = job.origin().chain_id,
                            batch_number = job.batch_number(),
                            kind = %job.kind(),
                            "Received job"
                        );
                        METRICS.pending_jobs[&job.kind()].inc_by(1);
                        self.pending_job = Some(job);
                        did_work = true;
                    }
                    None => debug!("No jobs available on any chain, waiting..."),
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

    /// One round-robin scan over the job servers, starting at the rotation
    /// cursor; returns the first job found and moves the cursor to the server
    /// after the one served. A per-server fetch error is logged and the scan
    /// moves on, so one unreachable job server can't stall the others; the
    /// caller sleeps one poll interval only when the whole scan comes up empty.
    fn fetch_job(&mut self) -> Option<WorkerJob> {
        let servers = self.clients.len();
        for offset in 0..servers {
            let server = (self.next_server + offset) % servers;
            let client = &self.clients[server];
            let fetched = match self.mode {
                ProverMode::FriOnly | ProverMode::FriSnark => client.fetch_fri_job(),
                ProverMode::SnarkOnly => client.fetch_snark_job(),
            };
            match fetched {
                Ok(Some(job)) => {
                    self.next_server = (server + 1) % servers;
                    return Some(job);
                }
                Ok(None) => {}
                Err(err) => warn!(
                    server,
                    server_url = client.server_url(),
                    ?err,
                    "Failed to fetch job from server, trying the next one"
                ),
            }
        }
        None
    }

    fn handle_prover_result(&mut self, outcome: ProverResult) -> Result<()> {
        let kind = match &outcome {
            Ok(o) => o.kind(),
            Err(f) => f.kind,
        };
        METRICS.pending_jobs[&kind].dec_by(1);
        let (origin, batch_number) = match outcome {
            Ok(ProofOutcome::Fri {
                origin,
                batch_number,
                proof,
                cycles_used,
            }) => {
                self.clients[origin.server].submit_fri(
                    origin.chain_id,
                    batch_number,
                    proof.as_ref(),
                    cycles_used,
                )?;
                // In `fri-snark` mode, the SNARK job needs the FRI proof, so we can set the new pending job immediately instead of waiting for the next fetch cycle. The in-memory `Proof` is fed directly to the SNARK pipeline without an extra encode/decode round trip.
                if self.mode == ProverMode::FriSnark {
                    // FRI jobs are processed serially, so the previous SNARK
                    // follow-up must have been drained before this one lands.
                    METRICS.pending_jobs[&ProofType::Snark].inc_by(1);
                    self.snark_followup = Some(WorkerJob::Snark {
                        origin,
                        batch_number,
                        proof,
                    });
                }
                (origin, batch_number)
            }
            Ok(ProofOutcome::Snark {
                origin,
                batch_number,
                proof,
            }) => {
                self.clients[origin.server].submit_snark(origin.chain_id, batch_number, proof)?;
                (origin, batch_number)
            }
            Err(failure) => {
                // Report the failure to the server so it can release the batch
                // for retry (bounded by the server's attempts limit) without
                // waiting for the proving timeout to elapse.
                warn!(
                    chain_id = failure.origin.chain_id,
                    batch_number = failure.batch_number,
                    kind = %failure.kind,
                    reason = %failure.reason,
                    "Prover job failed; reporting to server",
                );
                let client = &self.clients[failure.origin.server];
                match failure.kind {
                    ProofType::Fri => client.submit_fri_error(
                        failure.origin.chain_id,
                        failure.batch_number,
                        &failure.reason,
                    )?,
                    ProofType::Snark => client.submit_snark_error(
                        failure.origin.chain_id,
                        failure.batch_number,
                        &failure.reason,
                    )?,
                }
                info!(
                    chain_id = failure.origin.chain_id,
                    batch_number = failure.batch_number,
                    kind = %failure.kind,
                    "Reported failed proof to server",
                );
                return Ok(());
            }
        };
        info!(chain_id = origin.chain_id, batch_number, kind = %kind, "Successfully submitted proof");
        Ok(())
    }
}

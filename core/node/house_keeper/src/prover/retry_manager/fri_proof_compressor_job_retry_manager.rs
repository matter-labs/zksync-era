use std::time::Duration;

use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;

use crate::{periodic_job::PeriodicJob, prover::metrics::PROVER_FRI_METRICS};

/// `FriProofCompressorJobRetryManager` is a task that periodically queues stuck compressor jobs.
#[derive(Debug)]
pub struct FriProofCompressorJobRetryManager {
    pool: ConnectionPool<Prover>,
    max_attempts: u32,
    processing_timeout: Duration,
    retry_interval_ms: u64,
}

impl FriProofCompressorJobRetryManager {
    pub fn new(
        max_attempts: u32,
        processing_timeout: Duration,
        retry_interval_ms: u64,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        Self {
            max_attempts,
            processing_timeout,
            retry_interval_ms,
            pool,
        }
    }
}

#[async_trait]
impl PeriodicJob for FriProofCompressorJobRetryManager {
    const SERVICE_NAME: &'static str = "FriProofCompressorJobRetryManager";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri proof compressor job {:?}", stuck_job);
        }
        PROVER_FRI_METRICS
            .proof_compressor_requeued_jobs
            .inc_by(job_len as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

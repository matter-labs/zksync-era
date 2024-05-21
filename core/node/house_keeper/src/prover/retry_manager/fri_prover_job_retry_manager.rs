use std::time::Duration;

use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;

use crate::{periodic_job::PeriodicJob, prover::metrics::SERVER_METRICS};

/// `FriProverJobRetryManager` is a task that periodically queues stuck prover jobs.
#[derive(Debug)]
pub struct FriProverJobRetryManager {
    pool: ConnectionPool<Prover>,
    max_attempts: u32,
    processing_timeout: Duration,
    retry_interval_ms: u64,
}

impl FriProverJobRetryManager {
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
impl PeriodicJob for FriProverJobRetryManager {
    const SERVICE_NAME: &'static str = "FriProverJobRetryManager";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_prover_jobs_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri prover job {:?}", stuck_job);
        }
        SERVER_METRICS
            .prover_fri_requeued_jobs
            .inc_by(job_len as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

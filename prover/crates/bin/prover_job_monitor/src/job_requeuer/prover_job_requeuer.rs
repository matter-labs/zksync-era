use std::time::Duration;

use async_trait::async_trait;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};

use crate::metrics::PROVER_JOB_MONITOR_METRICS;

/// `ProverJobRequeuer` is a task that periodically requeues prover jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct ProverJobRequeuer {
    pool: ConnectionPool<Prover>,
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeout: Duration,
    /// time between each run
    run_interval_ms: u64,
}

impl ProverJobRequeuer {
    pub fn new(
        pool: ConnectionPool<Prover>,
        max_attempts: u32,
        processing_timeout: Duration,
        run_interval_ms: u64,
    ) -> Self {
        Self {
            pool,
            max_attempts,
            processing_timeout,
            run_interval_ms,
        }
    }
}

#[async_trait]
impl PeriodicJob for ProverJobRequeuer {
    const SERVICE_NAME: &'static str = "ProverJobRequeuer";

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
            tracing::info!("requeued circuit prover job {:?}", stuck_job);
        }
        PROVER_JOB_MONITOR_METRICS
            .requeued_circuit_prover_jobs
            .inc_by(job_len as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

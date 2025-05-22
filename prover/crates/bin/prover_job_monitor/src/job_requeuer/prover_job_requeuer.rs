use std::time::Duration;

use anyhow::Context;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;

use crate::metrics::SERVER_METRICS;

/// `ProverJobRequeuer` is a task that requeues prover jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct ProverJobRequeuer {
    pool: ConnectionPool<Prover>,
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeout: Duration,
}

impl ProverJobRequeuer {
    pub fn new(
        pool: ConnectionPool<Prover>,
        max_attempts: u32,
        processing_timeout: Duration,
    ) -> Self {
        Self {
            pool,
            max_attempts,
            processing_timeout,
        }
    }
}

#[async_trait::async_trait]
impl Task for ProverJobRequeuer {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        let stuck_jobs = connection
            .fri_prover_jobs_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;

        for stuck_job in stuck_jobs {
            tracing::info!("requeued circuit prover job {:?}", stuck_job);
            SERVER_METRICS.prover_fri_requeued_jobs[&stuck_job.chain_id.as_u64()].inc_by(1);
        }
        Ok(())
    }
}

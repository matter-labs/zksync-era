use std::time::Duration;

use async_trait::async_trait;
use zksync_prover_dal::{Connection, Prover, ProverDal};

use crate::{metrics::PROVER_JOB_MONITOR_METRICS, task_wiring::Task};

/// `ProverJobRequeuer` is a task that requeues prover jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct ProverJobRequeuer {
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeout: Duration,
}

impl ProverJobRequeuer {
    pub fn new(max_attempts: u32, processing_timeout: Duration) -> Self {
        Self {
            max_attempts,
            processing_timeout,
        }
    }
}

#[async_trait]
impl Task for ProverJobRequeuer {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        let stuck_jobs = connection
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
}

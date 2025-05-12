use std::time::Duration;

use anyhow::Context;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;

use crate::metrics::PROVER_FRI_METRICS;

/// `ProofCompressorJobRequeuer` is a task that requeues compressor jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct ProofCompressorJobRequeuer {
    pool: ConnectionPool<Prover>,
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeout: Duration,
}

impl ProofCompressorJobRequeuer {
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
impl Task for ProofCompressorJobRequeuer {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        let stuck_jobs = connection
            .fri_proof_compressor_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("requeued proof compressor job {:?}", stuck_job);
        }
        PROVER_FRI_METRICS
            .proof_compressor_requeued_jobs
            .inc_by(job_len as u64);
        Ok(())
    }
}

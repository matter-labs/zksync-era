use std::time::Duration;

use anyhow::Context;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;

use crate::metrics::PROVER_JOB_MONITOR_METRICS;

/// `ProverJobsArchiver` is a task that archives old finalized prover job.
///
/// The task will archive the `successful` prover jobs that have been done for a certain amount of time.
/// Note: This component speeds up provers, in their absence, queries would slow down due to state growth.
#[derive(Debug)]
pub struct ProverJobsArchiver {
    pool: ConnectionPool<Prover>,
    /// duration after which a prover job can be archived
    archive_jobs_after: Duration,
}

impl ProverJobsArchiver {
    pub fn new(pool: ConnectionPool<Prover>, archive_jobs_after: Duration) -> Self {
        Self {
            pool,
            archive_jobs_after,
        }
    }
}

#[async_trait::async_trait]
impl Task for ProverJobsArchiver {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        let archived_jobs = connection
            .fri_prover_jobs_dal()
            .archive_old_jobs(self.archive_jobs_after)
            .await;
        if archived_jobs > 0 {
            tracing::info!("Archived {:?} prover jobs", archived_jobs);
        }
        PROVER_JOB_MONITOR_METRICS
            .prover_job_archived
            .inc_by(archived_jobs as u64);
        Ok(())
    }
}

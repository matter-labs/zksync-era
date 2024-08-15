use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};

use crate::metrics::PROVER_JOB_MONITOR_METRICS;

/// `ProverJobsArchiver` is a task that periodically archives old finalized prover job.
/// The task will archive the `successful` prover jobs that have been done for a certain amount of time.
/// Note: These components speed up provers, in their absence, queries would slow down due to state growth.
#[derive(Debug)]
pub struct ProverJobsArchiver {
    pool: ConnectionPool<Prover>,
    /// time between each run
    run_interval_ms: u64,
    /// time after which a prover job can be archived
    archive_jobs_after_secs: u64,
}

impl ProverJobsArchiver {
    pub fn new(
        pool: ConnectionPool<Prover>,
        run_interval_ms: u64,
        archive_jobs_after_secs: u64,
    ) -> Self {
        Self {
            pool,
            run_interval_ms,
            archive_jobs_after_secs,
        }
    }
}

#[async_trait::async_trait]
impl PeriodicJob for ProverJobsArchiver {
    const SERVICE_NAME: &'static str = "ProverJobsArchiver";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let archived_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_prover_jobs_dal()
            .archive_old_jobs(self.archive_jobs_after_secs)
            .await;
        if archived_jobs > 0 {
            tracing::info!("Archived {:?} prover jobs", archived_jobs);
        }
        PROVER_JOB_MONITOR_METRICS
            .archived_prover_jobs
            .inc_by(archived_jobs as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

use zksync_dal::ConnectionPool;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{Prover, ProverDal};

use crate::prover::metrics::HOUSE_KEEPER_METRICS;

/// `FriProverJobsArchiver` is a task that periodically archives old finalized prover job.
/// The task will archive the `successful` prover jobs that have been done for a certain amount of time.
/// Note: These components speed up provers, in their absence, queries would become sub optimal.
#[derive(Debug)]
pub struct FriProverJobsArchiver {
    pool: ConnectionPool<Prover>,
    reporting_interval_ms: u64,
    archiving_interval_secs: u64,
}

impl FriProverJobsArchiver {
    pub fn new(
        pool: ConnectionPool<Prover>,
        reporting_interval_ms: u64,
        archiving_interval_secs: u64,
    ) -> Self {
        Self {
            pool,
            reporting_interval_ms,
            archiving_interval_secs,
        }
    }
}

#[async_trait::async_trait]
impl PeriodicJob for FriProverJobsArchiver {
    const SERVICE_NAME: &'static str = "FriProverJobsArchiver";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let archived_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_prover_jobs_dal()
            .archive_old_jobs(self.archiving_interval_secs)
            .await;
        tracing::info!("Archived {:?} fri prover jobs", archived_jobs);
        HOUSE_KEEPER_METRICS
            .prover_job_archived
            .inc_by(archived_jobs as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

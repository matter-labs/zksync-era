use prover_dal::{Prover, ProverDal};
use zksync_db_connection::connection_pool::ConnectionPool;

use crate::{house_keeper::periodic_job::PeriodicJob, metrics::HOUSE_KEEPER_METRICS};

#[derive(Debug)]
pub struct FriProverJobArchiver {
    pool: ConnectionPool<Prover>,
    reporting_interval_ms: u64,
    archiving_interval_secs: u64,
}

impl FriProverJobArchiver {
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
impl PeriodicJob for FriProverJobArchiver {
    const SERVICE_NAME: &'static str = "FriProverJobArchiver";

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

use std::time::Duration;

use zksync_dal::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};

use crate::{periodic_job::PeriodicJob, prover::metrics::HOUSE_KEEPER_METRICS};

/// `FriGpuProverArchiver` is a task that periodically archives old fri GPU prover records.
/// The task will archive the `dead` prover records that have not been updated for a certain amount of time.
/// Note: These components speed up provers, in their absence, queries would become sub optimal.
#[derive(Debug)]
pub struct FriGpuProverArchiver {
    pool: ConnectionPool<Prover>,
    archiving_interval_ms: u64,
    archive_prover_after_secs: u64,
}

impl FriGpuProverArchiver {
    pub fn new(
        pool: ConnectionPool<Prover>,
        archiving_interval_ms: u64,
        archive_prover_after_secs: u64,
    ) -> Self {
        Self {
            pool,
            archiving_interval_ms,
            archive_prover_after_secs,
        }
    }
}

#[async_trait::async_trait]
impl PeriodicJob for FriGpuProverArchiver {
    const SERVICE_NAME: &'static str = "FriGpuProverArchiver";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let archived_provers = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .archive_old_provers(Duration::from_secs(self.archive_prover_after_secs))
            .await;
        tracing::info!("Archived {:?} fri gpu prover records", archived_provers);
        HOUSE_KEEPER_METRICS
            .gpu_prover_archived
            .inc_by(archived_provers as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.archiving_interval_ms
    }
}

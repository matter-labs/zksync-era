use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};

use crate::metrics::PROVER_JOB_MONITOR_METRICS;

/// `GpuProverArchiver` is a task that periodically archives old fri GPU provers.
/// The task will archive the `dead` prover records that have not been updated for a certain amount of time.
/// Note: These components speed up provers, in their absence, queries would slow down due to state growth.
#[derive(Debug)]
pub struct GpuProverArchiver {
    pool: ConnectionPool<Prover>,
    /// time between each run
    run_interval_ms: u64,
    /// time after which a prover can be archived
    archive_prover_after_secs: u64,
}

impl GpuProverArchiver {
    pub fn new(
        pool: ConnectionPool<Prover>,
        run_interval_ms: u64,
        archive_prover_after_secs: u64,
    ) -> Self {
        Self {
            pool,
            run_interval_ms,
            archive_prover_after_secs,
        }
    }
}

#[async_trait::async_trait]
impl PeriodicJob for GpuProverArchiver {
    const SERVICE_NAME: &'static str = "GpuProverArchiver";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let archived_provers = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .archive_old_provers(self.archive_prover_after_secs)
            .await;
        if archived_provers > 0 {
            tracing::info!("Archived {:?} gpu provers", archived_provers);
        }
        PROVER_JOB_MONITOR_METRICS
            .archived_gpu_provers
            .inc_by(archived_provers as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

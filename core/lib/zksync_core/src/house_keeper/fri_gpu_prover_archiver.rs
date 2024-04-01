use prover_dal::{Prover, ProverDal};
use zksync_db_connection::connection_pool::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct FriGpuProverArchiver {
    pool: ConnectionPool<Prover>,
    reporting_interval_ms: u64,
    archiving_interval_secs: u64,
}

impl FriGpuProverArchiver {
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
impl PeriodicJob for FriGpuProverArchiver {
    const SERVICE_NAME: &'static str = "FriGpuProverArchiver";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let archived_provers = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .archive_old_provers(self.archiving_interval_secs)
            .await;
        tracing::info!("Archived {:?} fri gpu prover records", archived_provers);
        metrics::counter!(
            "server.prover_fri.archived_provers",
            archived_provers as u64
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

use std::time::Duration;

use zksync_prover_dal::{Connection, Prover, ProverDal};

use crate::{metrics::PROVER_JOB_MONITOR_METRICS, task_wiring::Task};

/// `GpuProverArchiver` is a task that archives old fri GPU provers.
/// The task will archive the `dead` prover records that have not been updated for a certain amount of time.
/// Note: This component speeds up provers, in their absence, queries would slow down due to state growth.
#[derive(Debug)]
pub struct GpuProverArchiver {
    /// duration after which a prover can be archived
    archive_prover_after: Duration,
}

impl GpuProverArchiver {
    pub fn new(archive_prover_after: Duration) -> Self {
        Self {
            archive_prover_after,
        }
    }
}

#[async_trait::async_trait]
impl Task for GpuProverArchiver {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        let archived_provers = connection
            .fri_gpu_prover_queue_dal()
            .archive_old_provers(self.archive_prover_after)
            .await;
        if archived_provers > 0 {
            tracing::info!("Archived {:?} gpu provers", archived_provers);
        }
        PROVER_JOB_MONITOR_METRICS
            .archived_gpu_provers
            .inc_by(archived_provers as u64);
        Ok(())
    }
}

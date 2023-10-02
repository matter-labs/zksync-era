use std::time::Duration;

use async_trait::async_trait;
use zksync_dal::ConnectionPool;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct ProverJobRetryManager {
    max_attempts: u32,
    processing_timeout: Duration,
    retry_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
}

impl ProverJobRetryManager {
    pub fn new(
        max_attempts: u32,
        processing_timeout: Duration,
        retry_interval_ms: u64,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            max_attempts,
            processing_timeout,
            retry_interval_ms,
            prover_connection_pool,
        }
    }
}

/// Invoked periodically to re-queue stuck prover jobs.
#[async_trait]
impl PeriodicJob for ProverJobRetryManager {
    const SERVICE_NAME: &'static str = "ProverJobRetryManager";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stuck_jobs = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .prover_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing prover job {:?}", stuck_job);
        }
        metrics::counter!("server.prover.requeued_jobs", job_len as u64);
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

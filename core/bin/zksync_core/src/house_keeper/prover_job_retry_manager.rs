use std::time::Duration;

use zksync_dal::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct ProverJobRetryManager {
    max_attempts: u32,
    processing_timeout: Duration,
    retry_interval_ms: u64,
}

impl ProverJobRetryManager {
    pub fn new(max_attempts: u32, processing_timeout: Duration, retry_interval_ms: u64) -> Self {
        Self {
            max_attempts,
            processing_timeout,
            retry_interval_ms,
        }
    }
}

/// Invoked periodically to re-queue stuck prover jobs.
impl PeriodicJob for ProverJobRetryManager {
    const SERVICE_NAME: &'static str = "ProverJobRetryManager";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let stuck_jobs = connection_pool
            .access_storage_blocking()
            .prover_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts);
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            vlog::info!("re-queuing prover job {:?}", stuck_job);
        }
        metrics::counter!("server.prover.requeued_jobs", job_len as u64);
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

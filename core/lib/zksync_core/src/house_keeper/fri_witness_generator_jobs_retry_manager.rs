use std::time::Duration;

use async_trait::async_trait;
use zksync_dal::ConnectionPool;
use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct FriWitnessGeneratorJobRetryManager {
    pool: ConnectionPool,
    max_attempts: u32,
    processing_timeout: Duration,
    retry_interval_ms: u64,
}

impl FriWitnessGeneratorJobRetryManager {
    pub fn new(
        max_attempts: u32,
        processing_timeout: Duration,
        retry_interval_ms: u64,
        pool: ConnectionPool,
    ) -> Self {
        Self {
            max_attempts,
            processing_timeout,
            retry_interval_ms,
            pool,
        }
    }

    pub async fn requeue_stuck_witness_inputs_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri witness input job {:?}", stuck_job);
        }
        metrics::counter!("server.witness_inputs_fri.requeued_jobs", job_len as u64);
    }

    pub async fn requeue_stuck_leaf_aggregations_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_leaf_aggregations_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri witness input job {:?}", stuck_job);
        }
        metrics::counter!(
            "server.leaf_aggregations_jobs_fri.requeued_jobs",
            job_len as u64
        );
    }

    pub async fn requeue_stuck_node_aggregations_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_node_aggregations_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri witness input job {:?}", stuck_job);
        }
        metrics::counter!(
            "server.node_aggregations_jobs_fri.requeued_jobs",
            job_len as u64
        );
    }

    pub async fn requeue_stuck_scheduler_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_scheduler_jobs(self.processing_timeout, self.max_attempts)
            .await;
        let job_len = stuck_jobs.len();
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing fri witness input job {:?}", stuck_job);
        }
        metrics::counter!("server.scheduler_jobs_fri.requeued_jobs", job_len as u64);
    }
}

/// Invoked periodically to re-queue stuck fri witness generator jobs.
#[async_trait]
impl PeriodicJob for FriWitnessGeneratorJobRetryManager {
    const SERVICE_NAME: &'static str = "FriWitnessGeneratorJobRetryManager";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.requeue_stuck_witness_inputs_jobs().await;
        self.requeue_stuck_leaf_aggregations_jobs().await;
        self.requeue_stuck_node_aggregations_jobs().await;
        self.requeue_stuck_scheduler_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

use async_trait::async_trait;
use zksync_dal::ConnectionPool;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct WaitingToQueuedWitnessJobMover {
    job_moving_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
}

impl WaitingToQueuedWitnessJobMover {
    pub fn new(job_mover_interval_ms: u64, prover_connection_pool: ConnectionPool) -> Self {
        Self {
            job_moving_interval_ms: job_mover_interval_ms,
            prover_connection_pool,
        }
    }

    async fn move_jobs(&mut self) {
        self.move_leaf_aggregation_jobs().await;
        self.move_node_aggregation_jobs().await;
        self.move_scheduler_jobs().await;
    }

    async fn move_leaf_aggregation_jobs(&mut self) {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_leaf_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            tracing::info!(
                "Marked leaf aggregation job for l1_batch {} as queued",
                l1_batch_number
            );
        }
        metrics::counter!(
            "server.leaf_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }

    async fn move_node_aggregation_jobs(&mut self) {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_node_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            tracing::info!(
                "Marking node aggregation job for l1_batch {} as queued",
                l1_batch_number
            );
        }
        metrics::counter!(
            "server.node_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }

    async fn move_scheduler_jobs(&mut self) {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_scheduler_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            tracing::info!(
                "Marking scheduler aggregation job for l1_batch {} as queued",
                l1_batch_number
            );
        }
        metrics::counter!(
            "server.scheduler_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }
}

#[async_trait]
impl PeriodicJob for WaitingToQueuedWitnessJobMover {
    const SERVICE_NAME: &'static str = "WaitingToQueuedWitnessJobMover";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.move_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.job_moving_interval_ms
    }
}

use async_trait::async_trait;
use zksync_dal::ConnectionPool;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct WaitingToQueuedFriWitnessJobMover {
    job_moving_interval_ms: u64,
    pool: ConnectionPool,
}

impl WaitingToQueuedFriWitnessJobMover {
    pub fn new(job_mover_interval_ms: u64, pool: ConnectionPool) -> Self {
        Self {
            job_moving_interval_ms: job_mover_interval_ms,
            pool,
        }
    }

    async fn move_leaf_aggregation_jobs(&mut self) {
        let mut conn = self.pool.access_storage().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_leaf_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for (l1_batch_number, circuit_id) in l1_batch_numbers {
            tracing::info!(
                "Marked fri leaf aggregation job for l1_batch {} and circuit_id {} as queued",
                l1_batch_number,
                circuit_id
            );
        }
        metrics::counter!(
            "server.node_fri_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }

    pub async fn move_node_aggregation_jobs_from_waiting_to_queued(
        &mut self,
    ) -> Vec<(i64, u8, u16)> {
        let mut conn = self.pool.access_storage().await.unwrap();
        let mut jobs = conn
            .fri_witness_generator_dal()
            .move_depth_zero_node_aggregation_jobs()
            .await;
        jobs.extend(
            conn.fri_witness_generator_dal()
                .move_depth_non_zero_node_aggregation_jobs()
                .await,
        );
        jobs
    }

    async fn move_node_aggregation_jobs(&mut self) {
        let l1_batch_numbers = self
            .move_node_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for (l1_batch_number, circuit_id, depth) in l1_batch_numbers {
            tracing::info!(
                "Marked fri node aggregation job for l1_batch {} and circuit_id {} depth {} as queued",
                l1_batch_number,
                circuit_id,
                depth
            );
        }
        metrics::counter!(
            "server.leaf_fri_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }
}

#[async_trait]
impl PeriodicJob for WaitingToQueuedFriWitnessJobMover {
    const SERVICE_NAME: &'static str = "WaitingToQueuedFriWitnessJobMover";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.move_leaf_aggregation_jobs().await;
        self.move_node_aggregation_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.job_moving_interval_ms
    }
}

use async_trait::async_trait;
use zksync_dal::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};

use crate::{periodic_job::PeriodicJob, prover::metrics::SERVER_METRICS};

#[derive(Debug)]
pub struct WaitingToQueuedFriWitnessJobMover {
    job_moving_interval_ms: u64,
    pool: ConnectionPool<Prover>,
}

impl WaitingToQueuedFriWitnessJobMover {
    pub fn new(job_mover_interval_ms: u64, pool: ConnectionPool<Prover>) -> Self {
        Self {
            job_moving_interval_ms: job_mover_interval_ms,
            pool,
        }
    }

    async fn move_leaf_aggregation_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
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

        SERVER_METRICS
            .node_fri_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(len as u64);
    }

    async fn move_node_aggregation_jobs_from_waiting_to_queued(&mut self) -> Vec<(i64, u8, u16)> {
        let mut conn = self.pool.connection().await.unwrap();
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
        SERVER_METRICS
            .leaf_fri_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(len as u64);
    }

    /// Marks recursion tip witness jobs as queued.
    /// The trigger condition is all final node proving jobs for the batch have been completed.
    async fn move_recursion_tip_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_recursion_tip_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked fri recursion tip witness job for l1_batch {} as queued",
                l1_batch_number,
            );
        }
        SERVER_METRICS
            .recursion_tip_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(l1_batch_numbers.len() as u64);
    }

    /// Marks scheduler witness jobs as queued.
    /// The trigger condition is the recursion tip proving job for the batch has been completed.
    async fn move_scheduler_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_scheduler_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked fri scheduler witness job for l1_batch {} as queued",
                l1_batch_number,
            );
        }
        SERVER_METRICS
            .scheduler_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(l1_batch_numbers.len() as u64);
    }
}

#[async_trait]
impl PeriodicJob for WaitingToQueuedFriWitnessJobMover {
    const SERVICE_NAME: &'static str = "WaitingToQueuedFriWitnessJobMover";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.move_leaf_aggregation_jobs().await;
        self.move_node_aggregation_jobs().await;
        self.move_recursion_tip_jobs().await;
        self.move_scheduler_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.job_moving_interval_ms
    }
}

use async_trait::async_trait;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};

use crate::metrics::{WitnessType, PROVER_JOB_MONITOR_METRICS};

/// `WitnessJobQueuer` is a task that periodically moves witness generator jobs from 'waiting_for_proofs' to 'queued'.
/// Note: this task is the backbone of scheduling/getting ready witness jobs to execute
#[derive(Debug)]
pub struct WitnessJobQueuer {
    pool: ConnectionPool<Prover>,
    /// time between each run
    run_interval_ms: u64,
}

impl WitnessJobQueuer {
    pub fn new(pool: ConnectionPool<Prover>, run_interval_ms: u64) -> Self {
        Self {
            pool,
            run_interval_ms,
        }
    }

    /// Marks leaf witness jobs as queued.
    /// The trigger condition is all prover jobs on round 0 for a given circuit, per batch, have been completed.
    async fn queue_leaf_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_leaf_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for (l1_batch_number, circuit_id) in l1_batch_numbers {
            tracing::info!(
                "Marked leaf job for l1_batch {} and circuit_id {} as queued.",
                l1_batch_number,
                circuit_id
            );
        }

        PROVER_JOB_MONITOR_METRICS.queued_witness_generator_jobs
            [&WitnessType::LeafWitnessGenerator]
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

    /// Marks node witness jobs as queued.
    /// The trigger condition is all prover jobs on round 1 (or 2 if recursing) for a given circuit, per batch, have been completed.
    async fn queue_node_jobs(&mut self) {
        let l1_batch_numbers = self
            .move_node_aggregation_jobs_from_waiting_to_queued()
            .await;
        let len = l1_batch_numbers.len();
        for (l1_batch_number, circuit_id, depth) in l1_batch_numbers {
            tracing::info!(
                "Marked node job for l1_batch {} and circuit_id {} at depth {} as queued.",
                l1_batch_number,
                circuit_id,
                depth
            );
        }
        PROVER_JOB_MONITOR_METRICS.queued_witness_generator_jobs
            [&WitnessType::NodeWitnessGenerator]
            .inc_by(len as u64);
    }

    /// Marks recursion tip witness jobs as queued.
    /// The trigger condition is all final node proving jobs for the batch have been completed.
    async fn queue_recursion_tip_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_recursion_tip_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked recursion tip job for l1_batch {} as queued.",
                l1_batch_number,
            );
        }
        PROVER_JOB_MONITOR_METRICS.queued_witness_generator_jobs
            [&WitnessType::RecursionTipWitnessGenerator]
            .inc_by(l1_batch_numbers.len() as u64);
    }

    /// Marks scheduler witness jobs as queued.
    /// The trigger condition is the recursion tip proving job for the batch has been completed.
    async fn queue_scheduler_jobs(&mut self) {
        let mut conn = self.pool.connection().await.unwrap();
        let l1_batch_numbers = conn
            .fri_witness_generator_dal()
            .move_scheduler_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked scheduler job for l1_batch {} as queued.",
                l1_batch_number,
            );
        }
        PROVER_JOB_MONITOR_METRICS.queued_witness_generator_jobs
            [&WitnessType::SchedulerWitnessGenerator]
            .inc_by(l1_batch_numbers.len() as u64);
    }
}

#[async_trait]
impl PeriodicJob for WitnessJobQueuer {
    const SERVICE_NAME: &'static str = "WitnessJobQueuer";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        // Note that there's no basic jobs here; basic witness generation is ready by the time it reaches prover subsystem.
        // It doesn't need to wait for any proof to start, as it is the process that maps the future execution (how many proofs and future witness generators).
        self.queue_leaf_jobs().await;
        self.queue_node_jobs().await;
        self.queue_recursion_tip_jobs().await;
        self.queue_scheduler_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

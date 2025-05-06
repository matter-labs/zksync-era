use anyhow::Context;
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::L1BatchId;

use crate::metrics::SERVER_METRICS;

/// `WitnessJobQueuer` is a task that moves witness generator jobs from 'waiting_for_proofs' to 'queued'.
/// Note: this task is the backbone of scheduling/getting ready witness jobs to execute.
#[derive(Debug)]
pub struct WitnessJobQueuer {
    pool: ConnectionPool<Prover>,
}

impl WitnessJobQueuer {
    pub fn new(pool: ConnectionPool<Prover>) -> Self {
        Self { pool }
    }

    /// Marks leaf witness jobs as queued.
    /// The trigger condition is all prover jobs on round 0 for a given circuit, per batch, have been completed.
    async fn queue_leaf_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let l1_batch_numbers = connection
            .fri_leaf_witness_generator_dal()
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

        SERVER_METRICS
            .leaf_fri_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(len as u64);
    }

    async fn move_node_aggregation_jobs_from_waiting_to_queued(
        &self,
        connection: &mut Connection<'_, Prover>,
    ) -> Vec<(L1BatchId, u8, u16)> {
        let mut jobs = connection
            .fri_node_witness_generator_dal()
            .move_depth_zero_node_aggregation_jobs()
            .await;
        jobs.extend(
            connection
                .fri_node_witness_generator_dal()
                .move_depth_non_zero_node_aggregation_jobs()
                .await,
        );
        jobs
    }

    /// Marks node witness jobs as queued.
    /// The trigger condition is all prover jobs on round 1 (or 2 if recursing) for a given circuit, per batch, have been completed.
    async fn queue_node_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let l1_batch_ids = self
            .move_node_aggregation_jobs_from_waiting_to_queued(connection)
            .await;
        let len = l1_batch_ids.len();
        for (batch_id, circuit_id, depth) in l1_batch_ids {
            tracing::info!(
                "Marked node job for l1_batch {} and circuit_id {} at depth {} as queued.",
                batch_id,
                circuit_id,
                depth
            );
        }
        SERVER_METRICS
            .node_fri_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(len as u64);
    }

    /// Marks recursion tip witness jobs as queued.
    /// The trigger condition is all final node proving jobs for the batch have been completed.
    async fn queue_recursion_tip_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let l1_batch_numbers = connection
            .fri_recursion_tip_witness_generator_dal()
            .move_recursion_tip_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked recursion tip job for l1_batch {} as queued.",
                l1_batch_number,
            );
        }
        SERVER_METRICS
            .recursion_tip_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(l1_batch_numbers.len() as u64);
    }

    /// Marks scheduler witness jobs as queued.
    /// The trigger condition is the recursion tip proving job for the batch has been completed.
    async fn queue_scheduler_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let l1_batch_numbers = connection
            .fri_scheduler_witness_generator_dal()
            .move_scheduler_jobs_from_waiting_to_queued()
            .await;
        for l1_batch_number in &l1_batch_numbers {
            tracing::info!(
                "Marked scheduler job for l1_batch {} as queued.",
                l1_batch_number,
            );
        }
        SERVER_METRICS
            .scheduler_witness_generator_waiting_to_queued_jobs_transitions
            .inc_by(l1_batch_numbers.len() as u64);
    }
}

#[async_trait::async_trait]
impl Task for WitnessJobQueuer {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        // Note that there's no basic jobs here; basic witness generation is ready by the time it reaches prover subsystem.
        // It doesn't need to wait for any proof to start, as it is the process that maps the future execution (how many proofs and future witness generators).
        self.queue_leaf_jobs(&mut connection).await;
        self.queue_node_jobs(&mut connection).await;
        self.queue_recursion_tip_jobs(&mut connection).await;
        self.queue_scheduler_jobs(&mut connection).await;
        Ok(())
    }
}

use anyhow::Context;
use zksync_config::configs::fri_witness_generator::WitnessGenerationTimeouts;
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::prover_dal::StuckJobs;

use crate::metrics::{WitnessType, SERVER_METRICS};

/// `WitnessGeneratorJobRequeuer` s a task that requeues witness generator jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct WitnessGeneratorJobRequeuer {
    pool: ConnectionPool<Prover>,
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeouts: WitnessGenerationTimeouts,
}

impl WitnessGeneratorJobRequeuer {
    pub fn new(
        pool: ConnectionPool<Prover>,
        max_attempts: u32,
        processing_timeouts: WitnessGenerationTimeouts,
    ) -> Self {
        Self {
            pool,
            max_attempts,
            processing_timeouts,
        }
    }

    fn emit_telemetry(&self, witness_type: WitnessType, stuck_jobs: &Vec<StuckJobs>) {
        for stuck_job in stuck_jobs {
            tracing::info!("requeued {:?} {:?}", witness_type, stuck_job);
            SERVER_METRICS.requeued_jobs[&(witness_type, stuck_job.chain_id.as_u64())].inc_by(1);
        }
    }

    async fn requeue_stuck_basic_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let stuck_jobs = connection
            .fri_basic_witness_generator_dal()
            .requeue_stuck_basic_jobs(self.processing_timeouts.basic(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::WitnessInputsFri, &stuck_jobs);
    }

    async fn requeue_stuck_leaf_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let stuck_jobs = connection
            .fri_leaf_witness_generator_dal()
            .requeue_stuck_leaf_jobs(self.processing_timeouts.leaf(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::LeafAggregationJobsFri, &stuck_jobs);
    }

    async fn requeue_stuck_node_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let stuck_jobs = connection
            .fri_node_witness_generator_dal()
            .requeue_stuck_node_jobs(self.processing_timeouts.node(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::NodeAggregationJobsFri, &stuck_jobs);
    }

    async fn requeue_stuck_recursion_tip_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let stuck_jobs = connection
            .fri_recursion_tip_witness_generator_dal()
            .requeue_stuck_recursion_tip_jobs(
                self.processing_timeouts.recursion_tip(),
                self.max_attempts,
            )
            .await;
        self.emit_telemetry(WitnessType::RecursionTipJobsFri, &stuck_jobs);
    }

    async fn requeue_stuck_scheduler_jobs(&self, connection: &mut Connection<'_, Prover>) {
        let stuck_jobs = connection
            .fri_scheduler_witness_generator_dal()
            .requeue_stuck_scheduler_jobs(self.processing_timeouts.scheduler(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::SchedulerJobsFri, &stuck_jobs);
    }
}

#[async_trait::async_trait]
impl Task for WitnessGeneratorJobRequeuer {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        self.requeue_stuck_basic_jobs(&mut connection).await;
        self.requeue_stuck_leaf_jobs(&mut connection).await;
        self.requeue_stuck_node_jobs(&mut connection).await;
        self.requeue_stuck_recursion_tip_jobs(&mut connection).await;
        self.requeue_stuck_scheduler_jobs(&mut connection).await;
        Ok(())
    }
}

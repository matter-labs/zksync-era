use async_trait::async_trait;
use zksync_config::configs::fri_witness_generator::WitnessGenerationTimeouts;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::prover_dal::StuckJobs;

use crate::metrics::{WitnessType, PROVER_JOB_MONITOR_METRICS};

/// `WitnessGeneratorJobRequeuer` s a task that periodically requeues witness generator jobs that have not made progress in a given unit of time.
#[derive(Debug)]
pub struct WitnessGeneratorJobRequeuer {
    pool: ConnectionPool<Prover>,
    /// max attempts before giving up on the job
    max_attempts: u32,
    /// the amount of time that must have passed before a job is considered to have not made progress
    processing_timeouts: WitnessGenerationTimeouts,
    /// time between each run
    run_interval_ms: u64,
}

impl WitnessGeneratorJobRequeuer {
    pub fn new(
        pool: ConnectionPool<Prover>,
        max_attempts: u32,
        processing_timeouts: WitnessGenerationTimeouts,
        run_interval_ms: u64,
    ) -> Self {
        Self {
            pool,
            max_attempts,
            processing_timeouts,
            run_interval_ms,
        }
    }

    fn emit_telemetry(&self, witness_type: WitnessType, stuck_jobs: &Vec<StuckJobs>) {
        for stuck_job in stuck_jobs {
            tracing::info!("requeued {:?} {:?}", witness_type, stuck_job);
        }
        PROVER_JOB_MONITOR_METRICS.requeued_witness_generator_jobs[&witness_type]
            .inc_by(stuck_jobs.len() as u64);
    }

    async fn requeue_stuck_basic_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_basic_jobs(self.processing_timeouts.basic(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::BasicWitnessGenerator, &stuck_jobs);
    }

    async fn requeue_stuck_leaf_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_leaf_jobs(self.processing_timeouts.leaf(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::LeafWitnessGenerator, &stuck_jobs);
    }

    async fn requeue_stuck_node_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_node_jobs(self.processing_timeouts.node(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::NodeWitnessGenerator, &stuck_jobs);
    }

    async fn requeue_stuck_recursion_tip_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_recursion_tip_jobs(
                self.processing_timeouts.recursion_tip(),
                self.max_attempts,
            )
            .await;
        self.emit_telemetry(WitnessType::RecursionTipWitnessGenerator, &stuck_jobs);
    }

    async fn requeue_stuck_scheduler_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_scheduler_jobs(self.processing_timeouts.scheduler(), self.max_attempts)
            .await;
        self.emit_telemetry(WitnessType::SchedulerWitnessGenerator, &stuck_jobs);
    }
}

#[async_trait]
impl PeriodicJob for WitnessGeneratorJobRequeuer {
    const SERVICE_NAME: &'static str = "WitnessGeneratorJobRequeuer";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.requeue_stuck_basic_jobs().await;
        self.requeue_stuck_leaf_jobs().await;
        self.requeue_stuck_node_jobs().await;
        self.requeue_stuck_recursion_tip_jobs().await;
        self.requeue_stuck_scheduler_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

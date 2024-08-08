use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_config::configs::fri_witness_generator::WitnessGenerationTimeouts;
use zksync_dal::ConnectionPool;
use zksync_types::prover_dal::StuckJobs;

use crate::{
    periodic_job::PeriodicJob,
    prover::metrics::{SERVER_METRICS, WitnessType},
};

/// `FriWitnessGeneratorJobRetryManager` is a task that periodically queues stuck prover jobs.
#[derive(Debug)]
pub struct FriWitnessGeneratorJobRetryManager {
    pool: ConnectionPool<Prover>,
    max_attempts: u32,
    processing_timeouts: WitnessGenerationTimeouts,
    retry_interval_ms: u64,
}

impl FriWitnessGeneratorJobRetryManager {
    pub fn new(
        max_attempts: u32,
        processing_timeouts: WitnessGenerationTimeouts,
        retry_interval_ms: u64,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        Self {
            max_attempts,
            processing_timeouts,
            retry_interval_ms,
            pool,
        }
    }

    pub fn emit_telemetry(&self, witness_type: &str, stuck_jobs: &Vec<StuckJobs>) {
        for stuck_job in stuck_jobs {
            tracing::info!("re-queuing {:?} {:?}", witness_type, stuck_job);
        }
        SERVER_METRICS.requeued_jobs[&WitnessType::from(witness_type)]
            .inc_by(stuck_jobs.len() as u64);
    }

    pub async fn requeue_stuck_witness_inputs_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_jobs(self.processing_timeouts.basic(), self.max_attempts)
            .await;
        self.emit_telemetry("witness_inputs_fri", &stuck_jobs);
    }

    pub async fn requeue_stuck_leaf_aggregations_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_leaf_aggregations_jobs(
                self.processing_timeouts.leaf(),
                self.max_attempts,
            )
            .await;
        self.emit_telemetry("leaf_aggregations_jobs_fri", &stuck_jobs);
    }

    pub async fn requeue_stuck_node_aggregations_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_node_aggregations_jobs(
                self.processing_timeouts.node(),
                self.max_attempts,
            )
            .await;
        self.emit_telemetry("node_aggregations_jobs_fri", &stuck_jobs);
    }

    pub async fn requeue_stuck_recursion_tip_jobs(&mut self) {
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
        self.emit_telemetry("recursion_tip_jobs_fri", &stuck_jobs);
    }

    pub async fn requeue_stuck_scheduler_jobs(&mut self) {
        let stuck_jobs = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .requeue_stuck_scheduler_jobs(self.processing_timeouts.scheduler(), self.max_attempts)
            .await;
        self.emit_telemetry("scheduler_jobs_fri", &stuck_jobs);
    }
}

#[async_trait]
impl PeriodicJob for FriWitnessGeneratorJobRetryManager {
    const SERVICE_NAME: &'static str = "FriWitnessGeneratorJobRetryManager";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.requeue_stuck_witness_inputs_jobs().await;
        self.requeue_stuck_leaf_aggregations_jobs().await;
        self.requeue_stuck_node_aggregations_jobs().await;
        self.requeue_stuck_recursion_tip_jobs().await;
        self.requeue_stuck_scheduler_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.retry_interval_ms
    }
}

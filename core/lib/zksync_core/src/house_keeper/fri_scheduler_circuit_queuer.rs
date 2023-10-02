use async_trait::async_trait;
use zksync_dal::ConnectionPool;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct SchedulerCircuitQueuer {
    queuing_interval_ms: u64,
    pool: ConnectionPool,
}

impl SchedulerCircuitQueuer {
    pub fn new(queuing_interval_ms: u64, pool: ConnectionPool) -> Self {
        Self {
            queuing_interval_ms,
            pool,
        }
    }

    pub async fn queue_scheduler_circuit_jobs(&mut self) {
        let mut conn = self.pool.access_storage().await.unwrap();
        let l1_batch_numbers = conn
            .fri_scheduler_dependency_tracker_dal()
            .get_l1_batches_ready_for_queuing()
            .await;
        let len = l1_batch_numbers.len();
        for &l1_batch_number in l1_batch_numbers.iter() {
            conn.fri_witness_generator_dal()
                .mark_scheduler_jobs_as_queued(l1_batch_number)
                .await;
            tracing::info!(
                "Marked fri scheduler aggregation job for l1_batch {} as queued",
                l1_batch_number,
            );
        }
        conn.fri_scheduler_dependency_tracker_dal()
            .mark_l1_batches_queued(l1_batch_numbers)
            .await;
        metrics::counter!(
            "server.scheduler_fri_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }
}

#[async_trait]
impl PeriodicJob for SchedulerCircuitQueuer {
    const SERVICE_NAME: &'static str = "SchedulerCircuitQueuer";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.queue_scheduler_circuit_jobs().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.queuing_interval_ms
    }
}

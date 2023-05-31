use zksync_dal::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct WaitingToQueuedWitnessJobMover {
    job_moving_interval_ms: u64,
}

impl WaitingToQueuedWitnessJobMover {
    pub fn new(job_mover_interval_ms: u64) -> Self {
        Self {
            job_moving_interval_ms: job_mover_interval_ms,
        }
    }

    fn move_jobs(&mut self, pool: ConnectionPool) {
        self.move_leaf_aggregation_jobs(pool.clone());
        self.move_node_aggregation_jobs(pool.clone());
        self.move_scheduler_jobs(pool);
    }

    fn move_leaf_aggregation_jobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_leaf_aggregation_jobs_from_waiting_to_queued();
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            vlog::info!(
                "Marked leaf aggregation job for l1_batch {} as queued",
                l1_batch_number
            );
        }
        metrics::counter!(
            "server.leaf_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }

    fn move_node_aggregation_jobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_node_aggregation_jobs_from_waiting_to_queued();
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            vlog::info!(
                "Marking node aggregation job for l1_batch {} as queued",
                l1_batch_number
            );
        }
        metrics::counter!(
            "server.node_witness_generator.waiting_to_queued_jobs_transitions",
            len as u64
        );
    }

    fn move_scheduler_jobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batch_numbers = conn
            .witness_generator_dal()
            .move_scheduler_jobs_from_waiting_to_queued();
        let len = l1_batch_numbers.len();
        for l1_batch_number in l1_batch_numbers {
            vlog::info!(
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

impl PeriodicJob for WaitingToQueuedWitnessJobMover {
    const SERVICE_NAME: &'static str = "WaitingToQueuedWitnessJobMover";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        self.move_jobs(connection_pool);
    }

    fn polling_interval_ms(&self) -> u64 {
        self.job_moving_interval_ms
    }
}

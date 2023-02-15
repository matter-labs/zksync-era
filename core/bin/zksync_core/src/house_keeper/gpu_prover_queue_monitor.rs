use zksync_dal::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug, Default)]
pub struct GpuProverQueueMonitor {}

/// Invoked periodically to push prover job statistics to Prometheus
/// Note: these values will be used for auto-scaling circuit-synthesizer
impl PeriodicJob for GpuProverQueueMonitor {
    const SERVICE_NAME: &'static str = "GpuProverQueueMonitor";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let free_prover_instance_count = connection_pool
            .access_storage_blocking()
            .gpu_prover_queue_dal()
            .get_count_of_jobs_ready_for_processing();
        vlog::info!(
            "Found {} free circuit synthesizer jobs",
            free_prover_instance_count
        );

        metrics::gauge!(
            "server.circuit_synthesizer.jobs",
            free_prover_instance_count as f64,
            "type" => "queued"
        );
    }
}

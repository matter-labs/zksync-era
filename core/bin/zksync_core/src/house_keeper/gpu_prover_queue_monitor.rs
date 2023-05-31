use zksync_dal::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct GpuProverQueueMonitor {
    synthesizer_per_gpu: u16,
    reporting_interval_ms: u64,
}

impl GpuProverQueueMonitor {
    pub fn new(synthesizer_per_gpu: u16, reporting_interval_ms: u64) -> Self {
        Self {
            synthesizer_per_gpu,
            reporting_interval_ms,
        }
    }
}

/// Invoked periodically to push prover job statistics to Prometheus
/// Note: these values will be used for auto-scaling circuit-synthesizer
impl PeriodicJob for GpuProverQueueMonitor {
    const SERVICE_NAME: &'static str = "GpuProverQueueMonitor";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let prover_gpu_count_per_region_zone = connection_pool
            .access_storage_blocking()
            .gpu_prover_queue_dal()
            .get_prover_gpu_count_per_region_zone();

        for ((region, zone), num_gpu) in prover_gpu_count_per_region_zone {
            let synthesizers = self.synthesizer_per_gpu as u64 * num_gpu;
            if synthesizers > 0 {
                vlog::info!(
                    "Would be spawning {} circuit synthesizers in region {} zone {}",
                    synthesizers,
                    region,
                    zone
                );
            }
            metrics::gauge!(
                "server.circuit_synthesizer.jobs",
                synthesizers as f64,
                    "region" => region,
                    "zone" => zone,
                "type" => "queued"
            );
        }
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

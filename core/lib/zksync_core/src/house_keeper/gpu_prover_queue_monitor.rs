use async_trait::async_trait;
use zksync_dal::ConnectionPool;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct GpuProverQueueMonitor {
    synthesizer_per_gpu: u16,
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
}

impl GpuProverQueueMonitor {
    pub fn new(
        synthesizer_per_gpu: u16,
        reporting_interval_ms: u64,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            synthesizer_per_gpu,
            reporting_interval_ms,
            prover_connection_pool,
        }
    }
}

/// Invoked periodically to push prover job statistics to Prometheus
/// Note: these values will be used for auto-scaling circuit-synthesizer
#[async_trait]
impl PeriodicJob for GpuProverQueueMonitor {
    const SERVICE_NAME: &'static str = "GpuProverQueueMonitor";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let prover_gpu_count_per_region_zone = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .gpu_prover_queue_dal()
            .get_prover_gpu_count_per_region_zone()
            .await;

        for ((region, zone), num_gpu) in prover_gpu_count_per_region_zone {
            let synthesizers = self.synthesizer_per_gpu as u64 * num_gpu;
            if synthesizers > 0 {
                tracing::info!(
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
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

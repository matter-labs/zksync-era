use async_trait::async_trait;
use zksync_dal::{fri_prover_dal::types::JobCountStatistics, ConnectionPool};

use crate::house_keeper::periodic_job::PeriodicJob;

const PROOF_COMPRESSOR_SERVICE_NAME: &str = "proof_compressor";

#[derive(Debug)]
pub struct FriProofCompressorStatsReporter {
    reporting_interval_ms: u64,
    pool: ConnectionPool,
}

impl FriProofCompressorStatsReporter {
    pub fn new(reporting_interval_ms: u64, pool: ConnectionPool) -> Self {
        Self {
            reporting_interval_ms,
            pool,
        }
    }

    async fn get_job_statistics(pool: &ConnectionPool) -> JobCountStatistics {
        pool.access_storage()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_jobs_stats()
            .await
    }
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for auto-scaling proof compressor
#[async_trait]
impl PeriodicJob for FriProofCompressorStatsReporter {
    const SERVICE_NAME: &'static str = "ProofCompressorStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats = Self::get_job_statistics(&self.pool).await;

        if stats.queued > 0 {
            tracing::info!(
                "Found {} free {} in progress proof compressor jobs",
                stats.queued,
                stats.in_progress
            );
        }

        metrics::gauge!(
            format!("prover_fri.{}.jobs", PROOF_COMPRESSOR_SERVICE_NAME),
            stats.queued as f64,
            "type" => "queued"
        );

        metrics::gauge!(
            format!("prover_fri.{}.jobs", PROOF_COMPRESSOR_SERVICE_NAME),
            stats.in_progress as f64,
            "type" => "in_progress"
        );

        let oldest_not_compressed_batch = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_oldest_not_compressed_batch()
            .await;

        if let Some(l1_batch_number) = oldest_not_compressed_batch {
            metrics::gauge!(
                format!(
                    "prover_fri.{}.oldest_not_compressed_batch",
                    PROOF_COMPRESSOR_SERVICE_NAME
                ),
                l1_batch_number.0 as f64
            );
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

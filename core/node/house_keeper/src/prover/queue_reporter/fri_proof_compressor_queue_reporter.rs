use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;
use zksync_types::{prover_dal::JobCountStatistics, ProtocolVersionId};

use crate::periodic_job::PeriodicJob;

const PROOF_COMPRESSOR_SERVICE_NAME: &str = "proof_compressor";

/// `FriProofCompressorQueueReporter` is a task that periodically reports compression jobs status.
/// Note: these values will be used for auto-scaling proof compressor
#[derive(Debug)]
pub struct FriProofCompressorQueueReporter {
    reporting_interval_ms: u64,
    pool: ConnectionPool<Prover>,
}

impl FriProofCompressorQueueReporter {
    pub fn new(reporting_interval_ms: u64, pool: ConnectionPool<Prover>) -> Self {
        Self {
            reporting_interval_ms,
            pool,
        }
    }

    async fn get_job_statistics(pool: &ConnectionPool<Prover>) -> JobCountStatistics {
        pool.connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_jobs_stats()
            .await
    }
}

#[async_trait]
impl PeriodicJob for FriProofCompressorQueueReporter {
    const SERVICE_NAME: &'static str = "FriProofCompressorQueueReporter";

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
            "type" => "queued",
            "protocol_version" => ProtocolVersionId::current_prover_version().to_string(),
        );

        metrics::gauge!(
            format!("prover_fri.{}.jobs", PROOF_COMPRESSOR_SERVICE_NAME),
            stats.in_progress as f64,
            "type" => "in_progress",
            "protocol_version" => ProtocolVersionId::current_prover_version().to_string(),
        );

        let oldest_not_compressed_batch = self
            .pool
            .connection()
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

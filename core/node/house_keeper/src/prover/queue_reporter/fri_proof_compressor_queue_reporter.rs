use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;
use zksync_types::{prover_dal::JobCountStatistics, ProtocolVersionId};

use crate::{
    metrics::{JobStatus, PROVER_FRI_METRICS},
    periodic_job::PeriodicJob,
    prover::metrics::JobStatus,
};

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

        PROVER_FRI_METRICS.proof_compressor_jobs[&(
            JobStatus::Queued,
            ProtocolVersionId::current_prover_version().to_string(),
        )]
            .set(stats.queued as u64);

        PROVER_FRI_METRICS.proof_compressor_jobs[&(
            JobStatus::InProgress,
            ProtocolVersionId::current_prover_version().to_string(),
        )]
            .set(stats.in_progress as u64);

        let oldest_not_compressed_batch = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_oldest_not_compressed_batch()
            .await;

        if let Some(l1_batch_number) = oldest_not_compressed_batch {
            PROVER_FRI_METRICS
                .proof_compressor_oldest_uncompressed_batch
                .set(l1_batch_number.0 as u64);
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}

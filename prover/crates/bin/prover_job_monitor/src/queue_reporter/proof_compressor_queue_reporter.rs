use std::collections::HashMap;

use async_trait::async_trait;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::JobCountStatistics};

use crate::metrics::{JobStatus, PROVER_JOB_MONITOR_METRICS};

/// `ProofCompressorQueueReporter` is a task that periodically reports compression jobs status.
/// Note: these values will be used for auto-scaling proof compressor
#[derive(Debug)]
pub struct ProofCompressorQueueReporter {
    pool: ConnectionPool<Prover>,
    /// time between each run
    run_interval_ms: u64,
}

impl ProofCompressorQueueReporter {
    pub fn new(pool: ConnectionPool<Prover>, run_interval_ms: u64) -> Self {
        Self {
            pool,
            run_interval_ms,
        }
    }

    async fn get_job_statistics(
        pool: &ConnectionPool<Prover>,
    ) -> HashMap<ProtocolSemanticVersion, JobCountStatistics> {
        pool.connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_jobs_stats()
            .await
    }
}

#[async_trait]
impl PeriodicJob for ProofCompressorQueueReporter {
    const SERVICE_NAME: &'static str = "ProofCompressorQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats = Self::get_job_statistics(&self.pool).await;

        for (protocol_version, stats) in &stats {
            if stats.queued > 0 {
                tracing::info!(
                    "Found {} queued proof compressor jobs for protocol version {}.",
                    stats.queued,
                    protocol_version
                );
            }
            if stats.in_progress > 0 {
                tracing::info!(
                    "Found {} in progress proof compressor jobs for protocol version {}.",
                    stats.in_progress,
                    protocol_version
                );
            }

            PROVER_JOB_MONITOR_METRICS.proof_compressor_jobs
                [&(JobStatus::Queued, protocol_version.to_string())]
                .set(stats.queued as u64);

            PROVER_JOB_MONITOR_METRICS.proof_compressor_jobs
                [&(JobStatus::InProgress, protocol_version.to_string())]
                .set(stats.in_progress as u64);
        }

        let oldest_not_compressed_batch = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_oldest_not_compressed_batch()
            .await;

        if let Some(l1_batch_number) = oldest_not_compressed_batch {
            PROVER_JOB_MONITOR_METRICS
                .oldest_uncompressed_batch
                .set(l1_batch_number.0 as u64);
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}

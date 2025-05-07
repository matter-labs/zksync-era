use std::collections::HashMap;

use anyhow::Context;
use zksync_prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::JobCountStatistics};

use crate::metrics::{JobStatus, PROVER_FRI_METRICS};

/// `ProofCompressorQueueReporter` is a task that reports compression jobs status.
/// Note: these values will be used for auto-scaling proof compressor.
#[derive(Debug)]
pub struct ProofCompressorQueueReporter {
    pool: ConnectionPool<Prover>,
}

impl ProofCompressorQueueReporter {
    pub fn new(pool: ConnectionPool<Prover>) -> Self {
        Self { pool }
    }

    async fn get_job_statistics(
        connection: &mut Connection<'_, Prover>,
    ) -> HashMap<ProtocolSemanticVersion, JobCountStatistics> {
        connection.fri_proof_compressor_dal().get_jobs_stats().await
    }
}

#[async_trait::async_trait]
impl Task for ProofCompressorQueueReporter {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        let stats = Self::get_job_statistics(&mut connection).await;

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

            PROVER_FRI_METRICS.proof_compressor_jobs
                [&(JobStatus::Queued, protocol_version.to_string())]
                .set(stats.queued as u64);

            PROVER_FRI_METRICS.proof_compressor_jobs
                [&(JobStatus::InProgress, protocol_version.to_string())]
                .set(stats.in_progress as u64);
        }

        let oldest_not_compressed_batch = connection
            .fri_proof_compressor_dal()
            .get_oldest_not_compressed_batch()
            .await;

        if let Some(batch_id) = oldest_not_compressed_batch {
            PROVER_FRI_METRICS
                .proof_compressor_oldest_uncompressed_batch
                .set(batch_id.batch_number().0 as u64);
        }

        Ok(())
    }
}

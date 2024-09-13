use std::collections::HashMap;

use async_trait::async_trait;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::JobCountStatistics};

use crate::{
    metrics::{JobStatus, PROVER_FRI_METRICS},
    task_wiring::Task,
};

/// `ProofCompressorQueueReporter` is a task that reports compression jobs status.
/// Note: these values will be used for auto-scaling proof compressor.
#[derive(Debug)]
pub struct ProofCompressorQueueReporter {}

impl ProofCompressorQueueReporter {
    async fn get_job_statistics(
        connection: &mut Connection<'_, Prover>,
    ) -> HashMap<ProtocolSemanticVersion, JobCountStatistics> {
        connection.fri_proof_compressor_dal().get_jobs_stats().await
    }
}

#[async_trait]
impl Task for ProofCompressorQueueReporter {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        let stats = Self::get_job_statistics(connection).await;

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

        if let Some(l1_batch_number) = oldest_not_compressed_batch {
            PROVER_FRI_METRICS
                .proof_compressor_oldest_uncompressed_batch
                .set(l1_batch_number.0 as u64);
        }

        Ok(())
    }
}

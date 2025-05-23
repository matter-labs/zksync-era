use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_job_processor::JobSaver;
use zksync_types::prover_dal::FriProverJobMetadata;

use crate::{
    metrics::WITNESS_VECTOR_GENERATOR_METRICS,
    types::witness_vector_generator_execution_output::WitnessVectorGeneratorExecutionOutput,
    witness_vector_generator::WitnessVectorGeneratorExecutor,
};

/// WitnessVectorGenerator job saver implementation.
/// On successful execution, sends data further to gpu circuit prover.
/// On error, marks the job as failed in database.
#[derive(Debug)]
pub struct WitnessVectorGeneratorJobSaver {
    connection_pool: ConnectionPool<Prover>,
    sender:
        tokio::sync::mpsc::Sender<(WitnessVectorGeneratorExecutionOutput, FriProverJobMetadata)>,
}

impl WitnessVectorGeneratorJobSaver {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        sender: tokio::sync::mpsc::Sender<(
            WitnessVectorGeneratorExecutionOutput,
            FriProverJobMetadata,
        )>,
    ) -> Self {
        Self {
            connection_pool,
            sender,
        }
    }
}

#[async_trait]
impl JobSaver for WitnessVectorGeneratorJobSaver {
    type ExecutorType = WitnessVectorGeneratorExecutor;

    #[tracing::instrument(
        name = "witness_vector_generator_save_job",
        skip_all,
        fields(l1_batch = % data.1.batch_id)
    )]
    async fn save_job_result(
        &self,
        data: (
            anyhow::Result<WitnessVectorGeneratorExecutionOutput>,
            FriProverJobMetadata,
        ),
    ) -> anyhow::Result<()> {
        let start_time = Instant::now();
        let (result, metadata) = data;
        match result {
            Ok(payload) => {
                tracing::info!(
                    "Started transferring witness vector generator job {}, on batch {}, for circuit {}, at round {}",
                    metadata.id,
                    metadata.batch_id,
                    metadata.circuit_id,
                    metadata.aggregation_round
                );
                if self.sender.send((payload, metadata)).await.is_err() {
                    tracing::warn!("circuit prover shut down prematurely");
                    return Ok(());
                }
                tracing::info!(
                    "Finished transferring witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
                    metadata.id,
                    metadata.batch_id,
                    metadata.circuit_id,
                    metadata.aggregation_round,
                    start_time.elapsed()
                );
                WITNESS_VECTOR_GENERATOR_METRICS
                    .transfer_time
                    .observe(start_time.elapsed());
            }
            Err(err) => {
                tracing::error!("Witness vector generation failed: {:?}", err);
                tracing::info!(
                    "Started saving failure for witness vector generator job {}, on batch {}, for circuit {}, at round {}",
                    metadata.id,
                    metadata.batch_id,
                    metadata.circuit_id,
                    metadata.aggregation_round
                );
                self.connection_pool
                    .connection()
                    .await
                    .context("failed to get db connection")?
                    .fri_prover_jobs_dal()
                    .save_proof_error(metadata.id, err.to_string())
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                tracing::info!(
                    "Finished saving failure for witness vector generator job {}, on batch {}, for circuit {}, at round {} in {:?}",
                    metadata.id,
                    metadata.batch_id,
                    metadata.circuit_id,
                    metadata.aggregation_round,
                    start_time.elapsed()
                );
                WITNESS_VECTOR_GENERATOR_METRICS
                    .save_time
                    .observe(start_time.elapsed());
            }
        }
        Ok(())
    }
}

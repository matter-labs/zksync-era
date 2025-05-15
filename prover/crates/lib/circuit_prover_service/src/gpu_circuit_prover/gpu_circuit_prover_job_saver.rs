use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_job_processor::JobSaver;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

use crate::{gpu_circuit_prover::GpuCircuitProverExecutor, metrics::CIRCUIT_PROVER_METRICS};

/// GpuCircuitProver job saver implementation.
/// Persists the job execution to database. In case of success, artifacts are uploaded to object store.
#[derive(Debug)]
pub struct GpuCircuitProverJobSaver {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
}

impl GpuCircuitProverJobSaver {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            protocol_version,
        }
    }
}

#[async_trait]
impl JobSaver for GpuCircuitProverJobSaver {
    type ExecutorType = GpuCircuitProverExecutor;

    #[tracing::instrument(
        name = "gpu_circuit_prover_job_saver",
        skip_all,
        fields(l1_batch = % data.1.batch_id)
    )]
    async fn save_job_result(
        &self,
        data: (anyhow::Result<FriProofWrapper>, FriProverJobMetadata),
    ) -> anyhow::Result<()> {
        let start_time = Instant::now();
        let (result, metadata) = data;
        tracing::info!(
            "Started saving gpu circuit prover job {}, on batch {}, for circuit {}, at round {}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round
        );

        match result {
            Ok(proof_wrapper) => {
                let mut connection = self
                    .connection_pool
                    .connection()
                    .await
                    .context("failed to get db connection")?;

                let is_scheduler_proof = metadata.is_scheduler_proof()?;

                let blob_url = self
                    .object_store
                    .put((metadata.id, metadata.batch_id.chain_id()), &proof_wrapper)
                    .await
                    .context("failed to upload to object store")?;

                let mut transaction = connection
                    .start_transaction()
                    .await
                    .context("failed to start db transaction")?;
                transaction
                    .fri_prover_jobs_dal()
                    .save_proof(
                        metadata.id,
                        metadata.batch_id.chain_id(),
                        metadata.pick_time.elapsed(),
                        &blob_url,
                    )
                    .await;

                if is_scheduler_proof {
                    transaction
                        .fri_proof_compressor_dal()
                        .insert_proof_compression_job(
                            metadata.batch_id,
                            &blob_url,
                            self.protocol_version,
                            metadata.batch_sealed_at,
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }
                transaction
                    .commit()
                    .await
                    .context("failed to commit db transaction")?;
            }
            Err(error) => {
                let error_message = error.to_string();
                tracing::error!("GPU circuit prover failed: {:?}", error);
                self.connection_pool
                    .connection()
                    .await
                    .context("failed to get db connection")?
                    .fri_prover_jobs_dal()
                    .save_proof_error(metadata.id, error_message)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
        };
        tracing::info!(
            "Finished saving gpu circuit prover job {}, on batch {}, for circuit {}, at round {} after {:?}",
            metadata.id,
            metadata.batch_id,
            metadata.circuit_id,
            metadata.aggregation_round,
            start_time.elapsed()
        );
        CIRCUIT_PROVER_METRICS
            .save_time
            .observe(start_time.elapsed());
        CIRCUIT_PROVER_METRICS
            .full_time
            .observe(metadata.pick_time.elapsed());
        Ok(())
    }
}

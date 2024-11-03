use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::FriProofWrapper;
use zksync_prover_job_processor::JobSaver;

use crate::gpu_circuit_prover::GpuCircuitProverExecutor;

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

    async fn save_result(
        &self,
        data: (anyhow::Result<FriProofWrapper>, FriProverJobMetadata),
    ) -> anyhow::Result<()> {
        tracing::info!("Started saving gpu circuit prover job");
        let (result, metadata) = data;

        match result {
            Ok(proof_wrapper) => {
                let mut connection = self
                    .connection_pool
                    .connection()
                    .await
                    .context("failed to get db connection")?;

                let is_scheduler_proof = metadata.is_scheduler_proof();

                let upload_time = Instant::now();
                let blob_url = self
                    .object_store
                    .put(metadata.id, &proof_wrapper)
                    .await
                    .context("failed to upload to object store")?;
                // CIRCUIT_PROVER_METRICS
                //     .artifact_upload_time
                //     .observe(upload_time.elapsed());

                let mut transaction = connection
                    .start_transaction()
                    .await
                    .context("failed to start db transaction")?;
                transaction
                    .fri_prover_jobs_dal()
                    .save_proof(metadata.id, Instant::now().elapsed(), &blob_url)
                    .await;
                if is_scheduler_proof {
                    transaction
                        .fri_proof_compressor_dal()
                        .insert_proof_compression_job(
                            metadata.block_number,
                            &blob_url,
                            self.protocol_version,
                        )
                        .await;
                }
                transaction
                    .commit()
                    .await
                    .context("failed to commit db transaction")?;

                //         tracing::info!(
                //     "Circuit Prover saved job {:?} after {:?}",
                //     job_id,
                //     time.elapsed()
                // );
            }
            Err(error) => {
                let error_message = error.to_string();
                println!("errored: {error_message:?}");
                self.connection_pool
                    .connection()
                    .await
                    .context("failed to get db connection")?
                    .fri_prover_jobs_dal()
                    .save_proof_error(metadata.id, error_message)
                    .await;
            }
        };
        tracing::info!("Finished saving gpu circuit prover job");
        Ok(())
    }
}

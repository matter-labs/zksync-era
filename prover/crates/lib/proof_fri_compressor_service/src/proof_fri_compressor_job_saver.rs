use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::{
    outputs::{L1BatchProofForL1, L1BatchProofForL1Key},
    CBOR,
};
use zksync_prover_job_processor::JobSaver;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_executor::ProofFriCompressorExecutor,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata,
};

/// ProofFriCompressor job saver implementation.
/// Persists the job execution to database. In case of success, artifacts are uploaded to object store.
#[derive(Debug)]
pub struct ProofFriCompressorJobSaver {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
}

impl ProofFriCompressorJobSaver {
    pub fn new(
        pool: ConnectionPool<Prover>,
        blob_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            pool,
            blob_store,
            protocol_version,
        }
    }
}

#[async_trait]
impl JobSaver for ProofFriCompressorJobSaver {
    type ExecutorType = ProofFriCompressorExecutor;

    #[tracing::instrument(
        name = "proof_fri_compressor_job_saver",
        skip_all,
        fields(l1_batch = % data.1.l1_batch_id)
    )]
    async fn save_job_result(
        &self,
        data: (
            anyhow::Result<L1BatchProofForL1<CBOR>>,
            ProofFriCompressorMetadata,
        ),
    ) -> anyhow::Result<()> {
        let start_time = Instant::now();
        let (result, metadata) = data;
        tracing::info!(
            "Started saving proof fri compressor job on batch {}",
            metadata.l1_batch_id,
        );

        match result {
            Ok(snark_wrapper_proof) => {
                let blob_url = self
                    .blob_store
                    .put(
                        L1BatchProofForL1Key::Prover((metadata.l1_batch_id, self.protocol_version)),
                        &snark_wrapper_proof,
                    )
                    .await
                    .context("Failed to save converted l1_batch_proof")?;

                self.pool
                    .connection()
                    .await
                    .context("failed to get db connection")?
                    .fri_proof_compressor_dal()
                    .mark_proof_compression_job_successful(
                        metadata.l1_batch_id,
                        start_time.elapsed(),
                        &blob_url,
                    )
                    .await;
            }
            Err(error) => {
                let error_message = error.to_string();
                tracing::error!("Proof Fri Compressor failed: {:?}", error);
                self.pool
                    .connection()
                    .await
                    .context("failed to get db connection")?
                    .fri_proof_compressor_dal()
                    .mark_proof_compression_job_failed(&error_message, metadata.l1_batch_id)
                    .await;
            }
        };
        tracing::info!(
            "Finished saving proof fri compressor job on batch {} after {:?}",
            metadata.l1_batch_id,
            start_time.elapsed()
        );
        PROOF_FRI_COMPRESSOR_METRICS
            .blob_save_time
            .observe(start_time.elapsed());

        Ok(())
    }
}

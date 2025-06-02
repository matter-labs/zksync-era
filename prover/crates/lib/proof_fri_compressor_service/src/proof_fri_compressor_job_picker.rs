use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use proof_compression_gpu::CompressorBlobStorage;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{get_current_pod_name, AuxOutputWitnessWrapper, FriProofWrapper};
use zksync_prover_job_processor::JobPicker;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_executor::ProofFriCompressorExecutor,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata,
    proof_fri_compressor_payload::ProofFriCompressorPayload,
};

/// ProofFriCompressor job picker implementation.
/// Checking FFLONK verification hash, picks job from database and gets data from object store.
pub struct ProofFriCompressorJobPicker {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    setup_data_cache: Arc<dyn CompressorBlobStorage>,
}

impl ProofFriCompressorJobPicker {
    pub fn new(
        pool: ConnectionPool<Prover>,
        blob_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        setup_data_cache: Arc<dyn CompressorBlobStorage>,
    ) -> Self {
        Self {
            pool,
            blob_store,
            protocol_version,
            setup_data_cache,
        }
    }
}

#[async_trait]
impl JobPicker for ProofFriCompressorJobPicker {
    type ExecutorType = ProofFriCompressorExecutor;

    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<Option<(ProofFriCompressorPayload, ProofFriCompressorMetadata)>> {
        let start_time = Instant::now();
        tracing::info!("Started picking proof fri compressor job",);

        let mut conn = self
            .pool
            .connection()
            .await
            .context("failed to get db connection")?;

        let l1_verifier_config = conn
            .fri_protocol_versions_dal()
            .get_l1_verifier_config()
            .await
            .map_err(|err| {
                anyhow::anyhow!("Failed to get L1 verifier config from database: {:?}", err)
            })?;

        let pod_name = get_current_pod_name();
        let Some(l1_batch_id) = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name, self.protocol_version)
            .await
        else {
            return Ok(None);
        };

        let Some(fri_proof_id) = conn
            .fri_prover_jobs_dal()
            .get_scheduler_proof_job_id(l1_batch_id)
            .await
        else {
            anyhow::bail!("Scheduler proof is missing from database for batch {l1_batch_id}");
        };

        let fri_proof: FriProofWrapper = self.blob_store.get((fri_proof_id, l1_batch_id.chain_id()))
            .await.with_context(|| format!("Failed to get fri proof from blob store for {l1_batch_id} with id {fri_proof_id}"))?;

        let aux_output_witness_wrapper: AuxOutputWitnessWrapper = self
            .blob_store
            .get(l1_batch_id)
            .await
            .context("Failed to get aggregation result coords from blob store")?;

        let setup_data_cache = self.setup_data_cache.clone();
        let payload = ProofFriCompressorPayload {
            scheduler_proof: match fri_proof {
                FriProofWrapper::Base(_) => {
                    anyhow::bail!("Must be a scheduler proof not base layer")
                }
                FriProofWrapper::Recursive(proof) => proof,
            },
            aux_output_witness_wrapper,
            setup_data_cache,
        };

        let metadata = ProofFriCompressorMetadata::new(l1_batch_id, l1_verifier_config);

        tracing::info!(
            "Finished picking proof fri compressor job on batch {} in {:?}",
            metadata.l1_batch_id,
            start_time.elapsed()
        );
        PROOF_FRI_COMPRESSOR_METRICS
            .blob_fetch_time
            .observe(start_time.elapsed());
        Ok(Some((payload, metadata)))
    }
}

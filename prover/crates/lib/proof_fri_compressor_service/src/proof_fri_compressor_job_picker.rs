use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{get_current_pod_name, FriProofWrapper};
use zksync_prover_job_processor::JobPicker;
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata,
    proof_fri_compressor_payload::ProofFriCompressorPayload, ProofFriCompressorExecutor,
};

/// ProofFriCompressor job picker implementation.
/// Picks job from database (via MetadataLoader) and gets data from object store.
#[derive(Debug)]
pub struct ProofFriCompressorJobPicker {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    is_fflonk: bool,
}

impl ProofFriCompressorJobPicker {
    pub fn new(
        pool: ConnectionPool<Prover>,
        blob_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        is_fflonk: bool,
    ) -> Self {
        Self {
            pool,
            blob_store,
            protocol_version,
            keystore,
            is_fflonk,
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
            .map_err(|_| anyhow::anyhow!("Failed to get L1 verifier config from database"))?;
        if l1_verifier_config.fflonk_snark_wrapper_vk_hash.is_none() && self.is_fflonk {
            anyhow::bail!("There was no FFLONK verification hash found in the database while trying to run compressor in FFLONK mode, aborting");
        }

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

        let scheduler_proof = ProofFriCompressorPayload {
            scheduler_proof: match fri_proof {
                FriProofWrapper::Base(_) => {
                    anyhow::bail!("Must be a scheduler proof not base layer")
                }
                FriProofWrapper::Recursive(proof) => proof,
            },
        };

        let keystore = self.keystore.clone();

        let metadata = ProofFriCompressorMetadata::new(l1_batch_id, self.is_fflonk, keystore);

        tracing::info!(
            "Finished picking proof fri compressor job on batch {} in {:?}",
            metadata.l1_batch_id,
            start_time.elapsed()
        );
        PROOF_FRI_COMPRESSOR_METRICS
            .blob_fetch_time
            .observe(start_time.elapsed());
        Ok(Some((scheduler_proof, metadata)))
    }
}

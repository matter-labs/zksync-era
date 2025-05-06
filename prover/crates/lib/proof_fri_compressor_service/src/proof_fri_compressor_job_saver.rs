use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use proof_compression_gpu::SnarkWrapperProof;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
    },
    AuxOutputWitnessWrapper,
};
use zksync_prover_interface::{
    outputs::{
        FflonkL1BatchProofForL1, L1BatchProofForL1, L1BatchProofForL1Key, PlonkL1BatchProofForL1,
    },
    CBOR,
};
use zksync_prover_job_processor::JobSaver;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata, ProofFriCompressorExecutor,
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

    fn aux_output_witness_to_array(
        aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
    ) -> [[u8; 32]; 4] {
        let mut array: [[u8; 32]; 4] = [[0; 32]; 4];

        for i in 0..32 {
            array[0][i] = aux_output_witness.l1_messages_linear_hash[i];
            array[1][i] = aux_output_witness.rollup_state_diff_for_compression[i];
            array[2][i] = aux_output_witness.bootloader_heap_initial_content[i];
            array[3][i] = aux_output_witness.events_queue_state[i];
        }
        array
    }
}

#[async_trait]
impl JobSaver for ProofFriCompressorJobSaver {
    type ExecutorType = ProofFriCompressorExecutor;

    #[tracing::instrument(
        name = "gpu_circuit_prover_job_saver",
        skip_all,
        fields(l1_batch = % data.1.l1_batch_id)
    )]
    async fn save_job_result(
        &self,
        data: (
            anyhow::Result<SnarkWrapperProof>,
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
                let aux_output_witness_wrapper: AuxOutputWitnessWrapper = self
                    .blob_store
                    .get(metadata.l1_batch_id)
                    .await
                    .context("Failed to get aggregation result coords from blob store")?;
                let aggregation_result_coords =
                    Self::aux_output_witness_to_array(aux_output_witness_wrapper.0);

                let l1_batch_proof: L1BatchProofForL1<CBOR> = match snark_wrapper_proof {
                    SnarkWrapperProof::Plonk(proof) => {
                        L1BatchProofForL1::new_plonk(PlonkL1BatchProofForL1 {
                            aggregation_result_coords,
                            scheduler_proof: proof,
                            protocol_version: self.protocol_version,
                        })
                    }
                    SnarkWrapperProof::FFfonk(proof) => {
                        L1BatchProofForL1::new_fflonk(FflonkL1BatchProofForL1 {
                            aggregation_result_coords,
                            scheduler_proof: proof,
                            protocol_version: self.protocol_version,
                        })
                    }
                };

                let blob_url = self
                    .blob_store
                    .put(
                        L1BatchProofForL1Key::Prover((metadata.l1_batch_id, self.protocol_version)),
                        &l1_batch_proof,
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

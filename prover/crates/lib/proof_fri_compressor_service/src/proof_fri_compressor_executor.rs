use std::time::Instant;

use proof_compression_gpu::{run_proof_chain, SnarkWrapper, SnarkWrapperProof};
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
};
use zksync_prover_interface::{
    outputs::{FflonkL1BatchProofForL1, L1BatchProofForL1, PlonkL1BatchProofForL1},
    CBOR,
};
use zksync_prover_job_processor::Executor;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata,
    proof_fri_compressor_payload::ProofFriCompressorPayload,
};

/// ProofFriCompressor executor implementation.
/// Compress the final proof with SNARK wrapper.
pub struct ProofFriCompressorExecutor {
    is_fflonk: bool,
    protocol_version: ProtocolSemanticVersion,
}

impl ProofFriCompressorExecutor {
    pub fn new(is_fflonk: bool, protocol_version: ProtocolSemanticVersion) -> Self {
        Self {
            is_fflonk,
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

impl Executor for ProofFriCompressorExecutor {
    type Input = ProofFriCompressorPayload;
    type Output = L1BatchProofForL1<CBOR>;
    type Metadata = ProofFriCompressorMetadata;

    #[tracing::instrument(
        name = "proof_fri_compressor_executor",
        skip_all,
        fields(l1_batch = % metadata.l1_batch_id)
    )]
    fn execute(
        &self,
        input: Self::Input,
        metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        let start_time = Instant::now();
        tracing::info!(
            "Started executing proof fri compressor job on batch {}",
            metadata.l1_batch_id
        );

        if metadata
            .l1_verifier_config
            .fflonk_snark_wrapper_vk_hash
            .is_none()
            && self.is_fflonk
        {
            anyhow::bail!("There was no FFLONK verification hash found in the database while trying to run compressor in FFLONK mode, aborting");
        }

        let snark_wrapper_mode = if self.is_fflonk {
            SnarkWrapper::FFfonk
        } else {
            SnarkWrapper::Plonk
        };

        let setup_data_cache = input.setup_data_cache;

        let proof_wrapper = run_proof_chain(
            snark_wrapper_mode,
            setup_data_cache,
            input.scheduler_proof.into_inner(),
        );

        let aggregation_result_coords =
            Self::aux_output_witness_to_array(input.aux_output_witness_wrapper.0);

        let l1_batch_proof: L1BatchProofForL1<CBOR> = match proof_wrapper {
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

        PROOF_FRI_COMPRESSOR_METRICS
            .compression_time
            .observe(start_time.elapsed());
        tracing::info!(
            "Finished proof fri compression for job: {:?} took: {:?}",
            metadata.l1_batch_id,
            start_time.elapsed()
        );
        Ok(l1_batch_proof)
    }
}

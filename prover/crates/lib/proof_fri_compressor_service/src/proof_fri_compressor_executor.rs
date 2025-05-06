use std::time::Instant;
use zksync_prover_job_processor::Executor;
use crate::{
    metrics::PROOF_FRI_COMPRESSOR_METRICS,
    proof_fri_compressor_metadata::ProofFriCompressorMetadata, 
    proof_fri_compressor_payload::ProofFriCompressorPayload
};
use proof_compression_gpu::run_proof_chain;
use proof_compression_gpu::SnarkWrapper;
use proof_compression_gpu::SnarkWrapperProof;

/// ProofFriCompressor executor implementation.
/// Generates circuit proof & verifies it.
/// NOTE: It requires prover context, which is the way Shivini allocates VRAM.
pub struct ProofFriCompressorExecutor;

impl Executor for ProofFriCompressorExecutor {
    type Input = ProofFriCompressorPayload;
    type Output = SnarkWrapperProof;
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

        let snark_wrapper_mode = if metadata.is_fflonk {
            SnarkWrapper::FFfonk
        } else {
            SnarkWrapper::Plonk
        };

        tracing::info!("RUNNING PROOF CHAIN");
        let proof_wrapper = run_proof_chain(
            snark_wrapper_mode,
            &metadata.keystore,
            input.scheduler_proof.into_inner(),
        );
        
        PROOF_FRI_COMPRESSOR_METRICS
            .compression_time
            .observe(start_time.elapsed());
        tracing::info!(
            "Finished proof fri compression for job: {:?} took: {:?}",
            metadata.l1_batch_id,
            start_time.elapsed()
        );
        Ok(proof_wrapper)
    }
}

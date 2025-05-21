use std::sync::Arc;

use proof_compression_gpu::SnarkWrapperSetup;
use zksync_prover_fri_types::{
    circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof,
    AuxOutputWitnessWrapper,
};

/// Payload used as input for Witness Vector Generator.
pub struct ProofFriCompressorPayload {
    pub scheduler_proof: ZkSyncRecursionLayerProof,
    pub aux_output_witness_wrapper: AuxOutputWitnessWrapper,
    pub setup_data_cache: Arc<SnarkWrapperSetup>,
}

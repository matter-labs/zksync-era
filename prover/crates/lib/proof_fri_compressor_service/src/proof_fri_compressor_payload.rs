use std::sync::Arc;

use proof_compression_gpu::CompressorBlobStorage;
use zksync_prover_fri_types::{
    circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof,
    AuxOutputWitnessWrapper,
};

/// Payload used as input for Proof FRI Compressor.
pub struct ProofFriCompressorPayload {
    pub scheduler_proof: ZkSyncRecursionLayerProof,
    pub aux_output_witness_wrapper: AuxOutputWitnessWrapper,
    pub setup_data_cache: Arc<dyn CompressorBlobStorage>,
}

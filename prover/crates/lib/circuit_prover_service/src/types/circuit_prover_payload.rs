use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;

use crate::types::circuit::Circuit;

/// Payload used as input for GPU circuit prover.
pub struct GpuCircuitProverPayload {
    pub circuit: Circuit,
    pub witness_vector: WitnessVec<GoldilocksField>,
    pub setup_data: Arc<GoldilocksGpuProverSetupData>,
}

use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};
use crate::types::setup_data::GoldilocksGpuProverSetupData;

use crate::types::circuit_wrapper::CircuitWrapper;

/// Payload used as input for GPU circuit prover.
pub struct GpuCircuitProverPayload {
    pub circuit_wrapper: CircuitWrapper,
    pub witness_vector: WitnessVec<GoldilocksField>,
    pub setup_data: Arc<GoldilocksGpuProverSetupData>,
}

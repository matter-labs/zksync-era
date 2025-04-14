use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};

use crate::types::circuit_wrapper::CircuitWrapper;

/// Witness vector generator output. Used as input for GPU circuit provers.
pub struct WitnessVectorGeneratorExecutionOutput {
    pub circuit_wrapper: CircuitWrapper,
    pub witness_vector: WitnessVec<GoldilocksField>,
}

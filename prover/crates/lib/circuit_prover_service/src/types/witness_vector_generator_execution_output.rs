use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};

use crate::types::circuit::Circuit;

/// Witness vector generator output. Used as input for GPU circuit provers.
pub struct WitnessVectorGeneratorExecutionOutput {
    pub circuit: Circuit,
    pub witness_vector: WitnessVec<GoldilocksField>,
}

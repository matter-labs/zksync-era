use zksync_prover_fri_types::circuit_definitions::boojum::{
    cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField,
};

use crate::types::circuit::Circuit;

pub struct WitnessVectorGeneratorExecutionOutput {
    pub circuit: Circuit,
    pub witness_vector: WitnessVec<GoldilocksField>,
}

impl WitnessVectorGeneratorExecutionOutput {
    pub fn new(circuit: Circuit, witness_vector: WitnessVec<GoldilocksField>) -> Self {
        Self {
            circuit,
            witness_vector,
        }
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    pub fn witness_vector(&self) -> &WitnessVec<GoldilocksField> {
        &self.witness_vector
    }
}

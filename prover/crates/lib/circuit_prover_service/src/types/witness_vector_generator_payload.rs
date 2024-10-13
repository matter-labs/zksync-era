use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;

use crate::types::circuit::Circuit;

pub struct WitnessVectorGeneratorPayload {
    circuit: Circuit,
    finalization_hints: Arc<FinalizationHintsForProver>,
}

impl WitnessVectorGeneratorPayload {
    pub fn new(circuit: Circuit, finalization_hints: Arc<FinalizationHintsForProver>) -> Self {
        Self {
            circuit,
            finalization_hints,
        }
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    pub fn finalization_hints(&self) -> Arc<FinalizationHintsForProver> {
        self.finalization_hints.clone()
    }
}

use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;

use crate::types::circuit_wrapper::CircuitWrapper;

/// Payload used as input for Witness Vector Generator.
pub struct WitnessVectorGeneratorPayload {
    pub circuit_wrapper: CircuitWrapper,
    pub finalization_hints: Arc<FinalizationHintsForProver>,
}

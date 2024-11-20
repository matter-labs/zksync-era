use std::sync::Arc;

use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;

use crate::types::circuit::Circuit;

/// Payload used as input for Witness Vector Generator.
pub struct WitnessVectorGeneratorPayload {
    pub circuit: Circuit,
    pub finalization_hints: Arc<FinalizationHintsForProver>,
}

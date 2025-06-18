use std::{collections::HashMap, sync::Arc};

use zksync_circuit_prover_service::types::setup_data::GoldilocksGpuProverSetupData;
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    ProverServiceDataKey,
};

// TODO: To be moved to circuit_prover_service lib & adjusted to new type idiom
// cache types
pub type SetupDataCache = HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>;
pub type FinalizationHintsCache = HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>;

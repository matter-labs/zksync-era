use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{block::L2BlockExecutionData, commitment::PubdataParams};
use zksync_vm_interface::{L1BatchEnv, SystemEnv};

/// Version 1 of the data used as input for the airbender verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1AirbenderVerifierInput {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub pubdata_params: PubdataParams,
}

/// Data used as input for the airbender verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum AirbenderVerifierInput {
    V1(V1AirbenderVerifierInput),
}

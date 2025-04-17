use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{block::L2BlockExecutionData, commitment::PubdataParams};
use zksync_vm_interface::{L1BatchEnv, SystemEnv};

/// Version 1 of the data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1TeeVerifierInput {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub pubdata_params: PubdataParams,
}

impl V1TeeVerifierInput {
    pub fn new(
        vm_run_data: VMRunWitnessInputData,
        merkle_paths: WitnessInputMerklePaths,
        l2_blocks_execution_data: Vec<L2BlockExecutionData>,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
    ) -> Self {
        V1TeeVerifierInput {
            vm_run_data,
            merkle_paths,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            pubdata_params,
        }
    }
}

/// Data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum TeeVerifierInput {
    /// `V0` suppresses warning about irrefutable `let...else` pattern
    V0,
    V1(V1TeeVerifierInput),
}

impl TeeVerifierInput {
    pub fn new(input: V1TeeVerifierInput) -> Self {
        TeeVerifierInput::V1(input)
    }
}

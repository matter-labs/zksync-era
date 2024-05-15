//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use multivm::interface::{L1BatchEnv, SystemEnv};
use serde::{Deserialize, Serialize};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    block::L2BlockExecutionData,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    L1BatchNumber, H256,
};

use crate::{
    inputs::PrepareBasicCircuitsJob,
    outputs::{L1BatchProofForL1, L1BatchTeeProofForL1},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationData {
    pub l1_batch_number: L1BatchNumber,
    pub data: PrepareBasicCircuitsJob,
    pub protocol_version: ProtocolSemanticVersion,
    pub l1_verifier_config: L1VerifierConfig,
    pub eip_4844_blobs: Eip4844Blobs,
}

/// ******************************************************************
/// Ugly copy-paste section
///
/// This line would do, but there are circular dependencies:
///   type TeeProofGenerationData = zksync_tee_verifier::TeeVerifierInput;

pub type TeeProofGenerationData = TeeVerifierInput;

/// Version 1 of the data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1TeeVerifierInput {
    prepare_basic_circuits_job: PrepareBasicCircuitsJob,
    l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
    used_contracts: Vec<(H256, Vec<u8>)>,
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
    pub fn new(
        prepare_basic_circuits_job: PrepareBasicCircuitsJob,
        l2_blocks_execution_data: Vec<L2BlockExecutionData>,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        used_contracts: Vec<(H256, Vec<u8>)>,
    ) -> Self {
        TeeVerifierInput::V1(V1TeeVerifierInput {
            prepare_basic_circuits_job,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            used_contracts,
        })
    }
}
/// ******************************************************************

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

pub type TeeProofGenerationDataRequest = ProofGenerationDataRequest;

#[derive(Debug, Serialize, Deserialize)]
pub enum GenericProofGenerationDataResponse<T> {
    Success(Option<Box<T>>),
    Error(String),
}

pub type ProofGenerationDataResponse = GenericProofGenerationDataResponse<ProofGenerationData>;
pub type TeeProofGenerationDataResponse =
    GenericProofGenerationDataResponse<TeeProofGenerationData>;

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofRequest {
    Proof(Box<L1BatchProofForL1>),
    TeeProof(Box<L1BatchTeeProofForL1>),
    // The proof generation was skipped due to sampling
    SkippedProofGeneration,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofResponse {
    Success,
    Error(String),
}

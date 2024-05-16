//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    protocol_version::{L1VerifierConfig, ProtocolVersionId},
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
    pub protocol_version_id: ProtocolVersionId,
    pub l1_verifier_config: L1VerifierConfig,
    pub eip_4844_blobs: Eip4844Blobs,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

pub type TeeProofGenerationDataRequest = ProofGenerationDataRequest;

#[derive(Debug, Serialize, Deserialize)]
pub enum GenericProofGenerationDataResponse<T> {
    Success(Option<Box<T>>),
    Error(String),
}

pub type ProofGenerationDataResponse = GenericProofGenerationDataResponse<ProofGenerationData>;

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

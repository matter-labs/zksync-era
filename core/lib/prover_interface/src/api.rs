//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    L1BatchNumber,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

pub type TeeProofGenerationDataRequest = ProofGenerationDataRequest;

#[derive(Debug, Serialize, Deserialize)]
pub enum GenericProofGenerationDataResponse<T> {
    Success(Option<Box<T>>),
    Error(String),
}

pub type ProofGenerationDataResponse = GenericProofGenerationDataResponse<ProofGenerationData>;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericSubmitProofRequest<T> {
    Proof(Box<T>),
    // The proof generation was skipped due to sampling
    SkippedProofGeneration,
}

pub type SubmitProofRequest = GenericSubmitProofRequest<L1BatchProofForL1>;
pub type SubmitTeeProofRequest = GenericSubmitProofRequest<L1BatchTeeProofForL1>;

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofResponse {
    Success,
    Error(String),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TeeType {
    Sgx,
}

#[test]
fn test_tee_proof_request_serialization() {
    let tee_proof = SubmitTeeProofRequest::Proof(Box::new(L1BatchTeeProofForL1 {
        signature: vec![0, 1, 2, 3, 4],
    }));
    let encoded = serde_json::to_string(&tee_proof).unwrap();
    assert_eq!(r#"{"Proof":{"signature":[0,1,2,3,4]}}"#, encoded);
    let decoded = serde_json::from_str(&encoded).unwrap();
    assert_eq!(tee_proof, decoded);
}

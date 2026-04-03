//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_types::tee_types::TeeType;

use crate::{inputs::AirbenderVerifierInput, outputs::L1BatchAirbenderProofForL1};

// Structs for holding data returned in HTTP responses

#[derive(Debug, Serialize, Deserialize)]
pub struct AirbenderProofGenerationDataResponse(pub Box<AirbenderVerifierInput>);

#[derive(Debug, Serialize, Deserialize)]
pub struct AirbenderPresentBatchesResponse {
    pub oldest_batch: Option<u32>,
    pub latest_batch: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitAirbenderProofResponse {
    Success,
    Error(String),
}
#[derive(Debug, Serialize, Deserialize)]
pub enum RegisterAirbenderAttestationResponse {
    Success,
}

// Structs to hold data necessary for making HTTP requests

#[derive(Debug, Serialize, Deserialize)]
pub struct AirbenderProofGenerationDataRequest {
    pub tee_type: TeeType,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitAirbenderProofRequest(pub Box<L1BatchAirbenderProofForL1>);

#[serde_as]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RegisterAirbenderAttestationRequest {
    #[serde_as(as = "Hex")]
    pub attestation: Vec<u8>,
    #[serde_as(as = "Hex")]
    pub pubkey: Vec<u8>,
}

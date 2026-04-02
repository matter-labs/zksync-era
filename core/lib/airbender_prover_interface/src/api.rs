//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};

use crate::inputs::AirbenderVerifierInput;

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

// Structs to hold data necessary for making HTTP requests

#[serde_as]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitAirbenderProofRequest {
    pub l1_batch_number: u32,
    #[serde_as(as = "Hex")]
    pub proof: Vec<u8>,
}

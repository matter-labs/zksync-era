//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_types::tee_types::TeeType;

use crate::{inputs::TeeVerifierInput, outputs::L1BatchTeeProofForL1};

// Structs for holding data returned in HTTP responses

#[derive(Debug, Serialize, Deserialize)]
pub struct TeeProofGenerationDataResponse(pub Box<TeeVerifierInput>);

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitTeeProofResponse {
    Success,
    Error(String),
}
#[derive(Debug, Serialize, Deserialize)]
pub enum RegisterTeeAttestationResponse {
    Success,
}

// Structs to hold data necessary for making HTTP requests

#[derive(Debug, Serialize, Deserialize)]
pub struct TeeProofGenerationDataRequest {
    pub tee_type: TeeType,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitTeeProofRequest(pub Box<L1BatchTeeProofForL1>);

#[serde_as]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RegisterTeeAttestationRequest {
    #[serde_as(as = "Hex")]
    pub attestation: Vec<u8>,
    #[serde_as(as = "Hex")]
    pub pubkey: Vec<u8>,
}

//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_prover_interface::outputs::SnarkWrapperProof;

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

/// SNARK input poll response (server -> prover). The `fri_proof` is the
/// hex-encoded bincode payload the FRI prover originally submitted. The
/// wrapper VK is resolved out-of-band at prover startup, so it isn't carried
/// here.
#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct AirbenderSnarkInputsResponse {
    pub l1_batch_number: u32,
    /// L2 chain id of the chain this server proves for. Lets a prover serving
    /// several chains attribute the job to the right chain in metrics/logs
    /// (FRI jobs carry it inside `system_env`; SNARK inputs have no other
    /// chain marker). Defaults to 0 when deserializing a response from a job
    /// server that predates the field, matching the prover's own fallback.
    #[serde(default)]
    pub chain_id: u64,
    #[serde_as(as = "Hex")]
    pub fri_proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitAirbenderSnarkProofResponse {
    Success,
    Error(String),
}

// Structs to hold data necessary for making HTTP requests

/// FRI submission payload. Carries either a proof (success) or an `error` (the prover could not
/// produce the proof), which releases the batch for retry — bounded by the configured attempts
/// limit — without waiting for the proving timeout to elapse. Exactly one of `proof`/`error` is
/// expected.
#[serde_as]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitAirbenderProofRequest {
    pub l1_batch_number: u32,
    pub prover_id: String,
    #[serde_as(as = "Option<Hex>")]
    #[serde(default)]
    pub proof: Option<Vec<u8>>,
    #[serde(default)]
    pub error: Option<String>,
    /// Guest cycles actually executed for the batch, as measured by the prover's
    /// RISC-V run. Optional so older provers that don't report it keep working;
    /// stored next to the sealer's prediction for cost-model calibration.
    #[serde(default)]
    pub cycles_used: Option<u64>,
}

/// SNARK submission payload. Like [`SubmitAirbenderProofRequest`], carries either a proof or an
/// `error`. The wrapper VK is resolved at prover startup and is not transmitted per proof.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitAirbenderSnarkProofRequest {
    pub l1_batch_number: u32,
    pub prover_id: String,
    #[serde(default)]
    pub snark_proof: Option<SnarkWrapperProof>,
    #[serde(default)]
    pub error: Option<String>,
}

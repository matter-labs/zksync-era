use crate::proofs::PrepareBasicCircuitsJob;
use serde::{Deserialize, Serialize};
use serde_with::base64::Base64;
use serde_with::serde_as;

use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationData {
    pub l1_batch_number: L1BatchNumber,
    pub data: PrepareBasicCircuitsJob,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub enum ProofGenerationDataResponse {
    Success(ProofGenerationData),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitProofRequest {
    // we have to use the RawProof until the SNARK Proof wrapper is implemented
    // https://linear.app/matterlabs/issue/CRY-1/implement-snark-wrapper-for-boojum
    pub proof: RawProof,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct RawProof {
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofResponse {
    Success,
    Error(String),
}

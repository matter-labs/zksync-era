//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use zksync_types::{
    protocol_version::{FriProtocolVersionId, L1VerifierConfig},
    L1BatchNumber,
};

use crate::{inputs::PrepareBasicCircuitsJob, outputs::L1BatchProofForL1};

const BLOB_CHUNK_SIZE: usize = 31;
const ELEMENTS_PER_4844_BLOCK: usize = 4096;

pub const EIP_4844_BLOB_SIZE: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

pub type Blob = Vec<u8>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Eip4844Blobs {
    pub blobs: Vec<Blob>,
}

impl From<Vec<u8>> for Eip4844Blobs {
    fn from(mut blobs: Vec<u8>) -> Self {
        let blob_size = blobs.len() / EIP_4844_BLOB_SIZE * EIP_4844_BLOB_SIZE;
        blobs.resize(blob_size, 0u8);
        Self {
            blobs: blobs
                .chunks(EIP_4844_BLOB_SIZE)
                .map(|chunk| chunk.into())
                .collect(),
        }
    }
}

impl From<Eip4844Blobs> for Vec<u8> {
    fn from(eip_4844_blobs: Eip4844Blobs) -> Self {
        eip_4844_blobs.blobs.iter().flatten().copied().collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationData {
    pub l1_batch_number: L1BatchNumber,
    pub data: PrepareBasicCircuitsJob,
    pub fri_protocol_version_id: FriProtocolVersionId,
    pub l1_verifier_config: L1VerifierConfig,
    pub eip_4844_blobs: Eip4844Blobs,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub enum ProofGenerationDataResponse {
    Success(Option<ProofGenerationData>),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofRequest {
    Proof(Box<L1BatchProofForL1>),
    // The proof generation was skipped due to sampling
    SkippedProofGeneration,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofResponse {
    Success,
    Error(String),
}

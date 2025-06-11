//! Prover and server subsystems communicate via the API.
//! This module defines the types used in the API.

use serde::{Deserialize, Serialize};
use zksync_object_store::{Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::{
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    L1BatchId, L1BatchNumber, L2ChainId,
};

use crate::{inputs::WitnessInputData, outputs::JsonL1BatchProofForL1};

// Structs for holding data returned in HTTP responses

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofGenerationData {
    pub l1_batch_number: L1BatchNumber,
    #[serde(default = "L2ChainId::zero")]
    pub chain_id: L2ChainId,
    #[serde(default = "chrono::Utc::now")]
    pub batch_sealed_at: chrono::DateTime<chrono::Utc>,
    pub witness_input_data: WitnessInputData,
    pub protocol_version: ProtocolSemanticVersion,
    pub l1_verifier_config: L1VerifierConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitProofGenerationDataResponse;

#[derive(Debug, Serialize, Deserialize)]
pub enum ProofGenerationDataResponse {
    Success(Option<Box<ProofGenerationData>>),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofResponse {
    Success,
    Error(String),
}

// Structs to hold data necessary for making HTTP requests

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofGenerationDataRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub enum SubmitProofRequest {
    Proof(Box<JsonL1BatchProofForL1>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PollGeneratedProofsRequest {
    pub l1_batch_id: L1BatchId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PollGeneratedProofsResponse {
    pub l1_batch_id: L1BatchId,
    pub proof: JsonL1BatchProofForL1,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyProofRequest(pub Box<JsonL1BatchProofForL1>);

impl StoredObject for ProofGenerationData {
    const BUCKET: Bucket = Bucket::PublicProofGenerationData;

    type Key<'a> = L1BatchId;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!(
            "proof_generation_data_{}_{}.cbor",
            key.batch_number().0,
            key.chain_id().as_u64()
        )
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|e| {
            BoxedError::from(format!("Failed to serialize ProofGenerationData: {e}"))
        })?;
        Ok(buf)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        ciborium::from_reader(&bytes[..]).map_err(|e| {
            BoxedError::from(format!("Failed to deserialize ProofGenerationData: {e}"))
        })
    }
}

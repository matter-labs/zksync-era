use ark_bn254::G1Affine;
use tonic::{transport::Error as TonicError, Status};
use zksync_eth_client::EnrichedClientError;

use super::blob_info::BlobQuorumParam;

/// Errors returned by this crate
#[derive(Debug, thiserror::Error)]
pub enum EigenClientError {
    #[error(transparent)]
    EthClient(#[from] EthClientError),
    #[error(transparent)]
    Verification(#[from] VerificationError),
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error(transparent)]
    BlobStatus(#[from] BlobStatusError),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Secp(#[from] secp256k1::Error),
    #[error(transparent)]
    Tonic(#[from] TonicError),
}

#[derive(Debug, thiserror::Error)]
pub enum CommunicationError {
    #[error(transparent)]
    Secp(#[from] secp256k1::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    GetBlobData(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, thiserror::Error)]
pub enum BlobStatusError {
    #[error(transparent)]
    Prost(#[from] prost::DecodeError),
    #[error(transparent)]
    Status(#[from] Status),
}

/// Errors specific to conversion
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {}

/// Errors for the EthClient
#[derive(Debug, thiserror::Error)]
pub enum EthClientError {
    #[error(transparent)]
    HTTPClient(#[from] reqwest::Error),
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error("RPC: {0}")]
    Rpc(String),
}

#[derive(Debug, thiserror::Error)]
pub enum KzgError {
    #[error("Kzg setup error: {0}")]
    Setup(String),
    #[error(transparent)]
    Internal(#[from] rust_kzg_bn254::errors::KzgError),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceManagerError {
    #[error(transparent)]
    EnrichedClient(#[from] EnrichedClientError),
    #[error("Decoding error: {0}")]
    Decoding(String),
}

/// Errors for the Verifier
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error(transparent)]
    ServiceManager(#[from] ServiceManagerError),
    #[error(transparent)]
    Kzg(#[from] KzgError),
    #[error("Wrong proof")]
    WrongProof,
    #[error("Different commitments: expected {expected:?}, got {actual:?}")]
    DifferentCommitments {
        expected: Box<G1Affine>,
        actual: Box<G1Affine>,
    },
    #[error("Different roots: expected {expected:?}, got {actual:?}")]
    DifferentRoots { expected: String, actual: String },
    #[error("Empty hashes")]
    EmptyHash,
    #[error("Different hashes: expected {expected:?}, got {actual:?}")]
    DifferentHashes { expected: String, actual: String },
    #[error("Wrong quorum params: {blob_quorum_params:?}")]
    WrongQuorumParams { blob_quorum_params: BlobQuorumParam },
    #[error("Quorum not confirmed")]
    QuorumNotConfirmed,
    #[error("Commitment not on curve: {0}")]
    CommitmentNotOnCurve(G1Affine),
    #[error("Commitment not on correct subgroup: {0}")]
    CommitmentNotOnCorrectSubgroup(G1Affine),
    #[error("Point download error: {0}")]
    PointDownloadError(String),
}

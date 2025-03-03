use std::fmt;

use super::{
    common::G1Commitment as DisperserG1Commitment,
    disperser::{
        BatchHeader as DisperserBatchHeader, BatchMetadata as DisperserBatchMetadata,
        BlobHeader as DisperserBlobHeader, BlobInfo as DisperserBlobInfo,
        BlobQuorumParam as DisperserBlobQuorumParam,
        BlobVerificationProof as DisperserBlobVerificationProof,
    },
};

#[derive(Debug)]
pub enum ConversionError {
    NotPresent,
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionError::NotPresent => write!(f, "Failed to convert BlobInfo"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct G1Commitment {
    pub x: Vec<u8>,
    pub y: Vec<u8>,
}

impl From<DisperserG1Commitment> for G1Commitment {
    fn from(value: DisperserG1Commitment) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlobQuorumParam {
    pub quorum_number: u32,
    pub adversary_threshold_percentage: u32,
    pub confirmation_threshold_percentage: u32,
    pub chunk_length: u32,
}

impl From<DisperserBlobQuorumParam> for BlobQuorumParam {
    fn from(value: DisperserBlobQuorumParam) -> Self {
        Self {
            quorum_number: value.quorum_number,
            adversary_threshold_percentage: value.adversary_threshold_percentage,
            confirmation_threshold_percentage: value.confirmation_threshold_percentage,
            chunk_length: value.chunk_length,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlobHeader {
    pub commitment: G1Commitment,
    pub data_length: u32,
    pub blob_quorum_params: Vec<BlobQuorumParam>,
}

impl TryFrom<DisperserBlobHeader> for BlobHeader {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobHeader) -> Result<Self, Self::Error> {
        let blob_quorum_params: Vec<BlobQuorumParam> = value
            .blob_quorum_params
            .iter()
            .map(|param| BlobQuorumParam::from(param.clone()))
            .collect();
        Ok(Self {
            commitment: G1Commitment::from(value.commitment.ok_or(ConversionError::NotPresent)?),
            data_length: value.data_length,
            blob_quorum_params,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BatchHeader {
    pub batch_root: Vec<u8>,
    pub quorum_numbers: Vec<u8>,
    pub quorum_signed_percentages: Vec<u8>,
    pub reference_block_number: u32,
}

impl From<DisperserBatchHeader> for BatchHeader {
    fn from(value: DisperserBatchHeader) -> Self {
        Self {
            batch_root: value.batch_root,
            quorum_numbers: value.quorum_numbers,
            quorum_signed_percentages: value.quorum_signed_percentages,
            reference_block_number: value.reference_block_number,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BatchMetadata {
    pub batch_header: BatchHeader,
    pub signatory_record_hash: Vec<u8>,
    pub fee: Vec<u8>,
    pub confirmation_block_number: u32,
    pub batch_header_hash: Vec<u8>,
}

impl TryFrom<DisperserBatchMetadata> for BatchMetadata {
    type Error = ConversionError;
    fn try_from(value: DisperserBatchMetadata) -> Result<Self, Self::Error> {
        Ok(Self {
            batch_header: BatchHeader::from(value.batch_header.ok_or(ConversionError::NotPresent)?),
            signatory_record_hash: value.signatory_record_hash,
            fee: value.fee,
            confirmation_block_number: value.confirmation_block_number,
            batch_header_hash: value.batch_header_hash,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlobVerificationProof {
    pub batch_id: u32,
    pub blob_index: u32,
    pub batch_medatada: BatchMetadata,
    pub inclusion_proof: Vec<u8>,
    pub quorum_indexes: Vec<u8>,
}

impl TryFrom<DisperserBlobVerificationProof> for BlobVerificationProof {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobVerificationProof) -> Result<Self, Self::Error> {
        Ok(Self {
            batch_id: value.batch_id,
            blob_index: value.blob_index,
            batch_medatada: BatchMetadata::try_from(
                value.batch_metadata.ok_or(ConversionError::NotPresent)?,
            )?,
            inclusion_proof: value.inclusion_proof,
            quorum_indexes: value.quorum_indexes,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlobInfo {
    pub blob_header: BlobHeader,
    pub blob_verification_proof: BlobVerificationProof,
}

impl TryFrom<DisperserBlobInfo> for BlobInfo {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            blob_header: BlobHeader::try_from(
                value.blob_header.ok_or(ConversionError::NotPresent)?,
            )?,
            blob_verification_proof: BlobVerificationProof::try_from(
                value
                    .blob_verification_proof
                    .ok_or(ConversionError::NotPresent)?,
            )?,
        })
    }
}

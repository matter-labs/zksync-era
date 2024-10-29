use std::fmt;

use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};

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
    NotPresentError,
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionError::NotPresentError => write!(f, "Failed to convert BlobInfo"),
        }
    }
}

#[derive(Debug)]
pub struct G1Commitment {
    pub x: Vec<u8>,
    pub y: Vec<u8>,
}

impl G1Commitment {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(&self.x.len().to_be_bytes());
        bytes.extend(&self.x);
        bytes.extend(&self.y.len().to_be_bytes());
        bytes.extend(&self.y);

        bytes
    }
}

impl Decodable for G1Commitment {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let x: Vec<u8> = rlp.val_at(0)?; // Decode first element as Vec<u8>
        let y: Vec<u8> = rlp.val_at(1)?; // Decode second element as Vec<u8>

        Ok(G1Commitment { x, y })
    }
}

impl Encodable for G1Commitment {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2);
        s.append(&self.x);
        s.append(&self.y);
    }
}

impl From<DisperserG1Commitment> for G1Commitment {
    fn from(value: DisperserG1Commitment) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug)]
pub struct BlobQuorumParam {
    pub quorum_number: u32,
    pub adversary_threshold_percentage: u32,
    pub confirmation_threshold_percentage: u32,
    pub chunk_length: u32,
}

impl BlobQuorumParam {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(&self.quorum_number.to_be_bytes());
        bytes.extend(&self.adversary_threshold_percentage.to_be_bytes());
        bytes.extend(&self.confirmation_threshold_percentage.to_be_bytes());
        bytes.extend(&self.chunk_length.to_be_bytes());

        bytes
    }
}

impl Decodable for BlobQuorumParam {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        Ok(BlobQuorumParam {
            quorum_number: rlp.val_at(0)?,
            adversary_threshold_percentage: rlp.val_at(1)?,
            confirmation_threshold_percentage: rlp.val_at(2)?,
            chunk_length: rlp.val_at(3)?,
        })
    }
}

impl Encodable for BlobQuorumParam {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4);
        s.append(&self.quorum_number);
        s.append(&self.adversary_threshold_percentage);
        s.append(&self.confirmation_threshold_percentage);
        s.append(&self.chunk_length);
    }
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

#[derive(Debug)]
pub struct BlobHeader {
    pub commitment: G1Commitment,
    pub data_length: u32,
    pub blob_quorum_params: Vec<BlobQuorumParam>,
}

impl BlobHeader {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(self.commitment.into_bytes());
        bytes.extend(&self.data_length.to_be_bytes());
        bytes.extend(&self.blob_quorum_params.len().to_be_bytes());

        for quorum in &self.blob_quorum_params {
            bytes.extend(quorum.into_bytes());
        }

        bytes
    }
}

impl Decodable for BlobHeader {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let commitment: G1Commitment = rlp.val_at(0)?;
        let data_length: u32 = rlp.val_at(1)?;
        let blob_quorum_params: Vec<BlobQuorumParam> = rlp.list_at(2)?;

        Ok(BlobHeader {
            commitment,
            data_length,
            blob_quorum_params,
        })
    }
}

impl Encodable for BlobHeader {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3);
        s.append(&self.commitment);
        s.append(&self.data_length);
        s.append_list(&self.blob_quorum_params);
    }
}

impl TryFrom<DisperserBlobHeader> for BlobHeader {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobHeader) -> Result<Self, Self::Error> {
        if value.commitment.is_none() {
            return Err(ConversionError::NotPresentError);
        }
        let blob_quorum_params: Vec<BlobQuorumParam> = value
            .blob_quorum_params
            .iter()
            .map(|param| BlobQuorumParam::from(param.clone()))
            .collect();
        let blob_quorum_params = blob_quorum_params;
        Ok(Self {
            commitment: G1Commitment::from(value.commitment.unwrap()),
            data_length: value.data_length,
            blob_quorum_params,
        })
    }
}

#[derive(Debug)]
pub struct BatchHeader {
    pub batch_root: Vec<u8>,
    pub quorum_numbers: Vec<u8>,
    pub quorum_signed_percentages: Vec<u8>,
    pub reference_block_number: u32,
}

impl BatchHeader {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(&self.batch_root.len().to_be_bytes());
        bytes.extend(&self.batch_root);
        bytes.extend(&self.quorum_numbers.len().to_be_bytes());
        bytes.extend(&self.quorum_numbers);
        bytes.extend(&self.quorum_signed_percentages.len().to_be_bytes());
        bytes.extend(&self.quorum_signed_percentages);
        bytes.extend(&self.reference_block_number.to_be_bytes());

        bytes
    }
}

impl Decodable for BatchHeader {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        Ok(BatchHeader {
            batch_root: rlp.val_at(0)?,
            quorum_numbers: rlp.val_at(1)?,
            quorum_signed_percentages: rlp.val_at(2)?,
            reference_block_number: rlp.val_at(3)?,
        })
    }
}

impl Encodable for BatchHeader {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(4);
        s.append(&self.batch_root);
        s.append(&self.quorum_numbers);
        s.append(&self.quorum_signed_percentages);
        s.append(&self.reference_block_number);
    }
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

#[derive(Debug)]
pub struct BatchMetadata {
    pub batch_header: BatchHeader,
    pub signatory_record_hash: Vec<u8>,
    pub fee: Vec<u8>,
    pub confirmation_block_number: u32,
    pub batch_header_hash: Vec<u8>,
}

impl BatchMetadata {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(self.batch_header.into_bytes());
        bytes.extend(&self.signatory_record_hash);
        bytes.extend(&self.confirmation_block_number.to_be_bytes());

        bytes
    }
}

impl Decodable for BatchMetadata {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let batch_header: BatchHeader = rlp.val_at(0)?;

        Ok(BatchMetadata {
            batch_header,
            signatory_record_hash: rlp.val_at(1)?,
            fee: rlp.val_at(2)?,
            confirmation_block_number: rlp.val_at(3)?,
            batch_header_hash: rlp.val_at(4)?,
        })
    }
}

impl Encodable for BatchMetadata {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5);
        s.append(&self.batch_header);
        s.append(&self.signatory_record_hash);
        s.append(&self.fee);
        s.append(&self.confirmation_block_number);
        s.append(&self.batch_header_hash);
    }
}

impl TryFrom<DisperserBatchMetadata> for BatchMetadata {
    type Error = ConversionError;
    fn try_from(value: DisperserBatchMetadata) -> Result<Self, Self::Error> {
        if value.batch_header.is_none() {
            return Err(ConversionError::NotPresentError);
        }
        Ok(Self {
            batch_header: BatchHeader::from(value.batch_header.unwrap()),
            signatory_record_hash: value.signatory_record_hash,
            fee: value.fee,
            confirmation_block_number: value.confirmation_block_number,
            batch_header_hash: value.batch_header_hash,
        })
    }
}

#[derive(Debug)]
pub struct BlobVerificationProof {
    pub batch_id: u32,
    pub blob_index: u32,
    pub batch_medatada: BatchMetadata,
    pub inclusion_proof: Vec<u8>,
    pub quorum_indexes: Vec<u8>,
}

impl BlobVerificationProof {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(&self.batch_id.to_be_bytes());
        bytes.extend(&self.blob_index.to_be_bytes());
        bytes.extend(self.batch_medatada.into_bytes());
        bytes.extend(&self.inclusion_proof.len().to_be_bytes());
        bytes.extend(&self.inclusion_proof);
        bytes.extend(&self.quorum_indexes.len().to_be_bytes());
        bytes.extend(&self.quorum_indexes);

        bytes
    }
}

impl Decodable for BlobVerificationProof {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        Ok(BlobVerificationProof {
            batch_id: rlp.val_at(0)?,
            blob_index: rlp.val_at(1)?,
            batch_medatada: rlp.val_at(2)?,
            inclusion_proof: rlp.val_at(3)?,
            quorum_indexes: rlp.val_at(4)?,
        })
    }
}

impl Encodable for BlobVerificationProof {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(5);
        s.append(&self.batch_id);
        s.append(&self.blob_index);
        s.append(&self.batch_medatada);
        s.append(&self.inclusion_proof);
        s.append(&self.quorum_indexes);
    }
}

impl TryFrom<DisperserBlobVerificationProof> for BlobVerificationProof {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobVerificationProof) -> Result<Self, Self::Error> {
        if value.batch_metadata.is_none() {
            return Err(ConversionError::NotPresentError);
        }
        Ok(Self {
            batch_id: value.batch_id,
            blob_index: value.blob_index,
            batch_medatada: BatchMetadata::try_from(value.batch_metadata.unwrap())?,
            inclusion_proof: value.inclusion_proof,
            quorum_indexes: value.quorum_indexes,
        })
    }
}

#[derive(Debug)]
pub struct BlobInfo {
    pub blob_header: BlobHeader,
    pub blob_verification_proof: BlobVerificationProof,
}

impl BlobInfo {
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        let blob_header_bytes = self.blob_header.into_bytes();
        bytes.extend(blob_header_bytes.len().to_be_bytes());
        bytes.extend(blob_header_bytes);
        let blob_verification_proof_bytes = self.blob_verification_proof.into_bytes();
        bytes.extend(blob_verification_proof_bytes);
        bytes
    }
}

impl Decodable for BlobInfo {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let blob_header: BlobHeader = rlp.val_at(0)?;
        let blob_verification_proof: BlobVerificationProof = rlp.val_at(1)?;

        Ok(BlobInfo {
            blob_header,
            blob_verification_proof,
        })
    }
}

impl Encodable for BlobInfo {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(2);
        s.append(&self.blob_header);
        s.append(&self.blob_verification_proof);
    }
}

impl TryFrom<DisperserBlobInfo> for BlobInfo {
    type Error = ConversionError;
    fn try_from(value: DisperserBlobInfo) -> Result<Self, Self::Error> {
        if value.blob_header.is_none() || value.blob_verification_proof.is_none() {
            return Err(ConversionError::NotPresentError);
        }
        Ok(Self {
            blob_header: BlobHeader::try_from(value.blob_header.unwrap())?,
            blob_verification_proof: BlobVerificationProof::try_from(
                value.blob_verification_proof.unwrap(),
            )?,
        })
    }
}

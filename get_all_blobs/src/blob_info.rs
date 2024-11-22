use std::fmt;

use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};

use crate::generated::{
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

#[derive(Debug, PartialEq, Clone)]
pub struct G1Commitment {
    pub x: Vec<u8>,
    pub y: Vec<u8>,
}

impl G1Commitment {
    pub fn to_bytes(&self) -> Vec<u8> {
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

#[derive(Debug, PartialEq, Clone)]
pub struct BlobQuorumParam {
    pub quorum_number: u32,
    pub adversary_threshold_percentage: u32,
    pub confirmation_threshold_percentage: u32,
    pub chunk_length: u32,
}

impl BlobQuorumParam {
    pub fn to_bytes(&self) -> Vec<u8> {
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

#[derive(Debug, PartialEq, Clone)]
pub struct BlobHeader {
    pub commitment: G1Commitment,
    pub data_length: u32,
    pub blob_quorum_params: Vec<BlobQuorumParam>,
}

impl BlobHeader {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(self.commitment.to_bytes());
        bytes.extend(&self.data_length.to_be_bytes());
        bytes.extend(&self.blob_quorum_params.len().to_be_bytes());

        for quorum in &self.blob_quorum_params {
            bytes.extend(quorum.to_bytes());
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
        Ok(Self {
            commitment: G1Commitment::from(value.commitment.unwrap()),
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

impl BatchHeader {
    pub fn to_bytes(&self) -> Vec<u8> {
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

#[derive(Debug, PartialEq, Clone)]
pub struct BatchMetadata {
    pub batch_header: BatchHeader,
    pub signatory_record_hash: Vec<u8>,
    pub fee: Vec<u8>,
    pub confirmation_block_number: u32,
    pub batch_header_hash: Vec<u8>,
}

impl BatchMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(self.batch_header.to_bytes());
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

#[derive(Debug, PartialEq, Clone)]
pub struct BlobVerificationProof {
    pub batch_id: u32,
    pub blob_index: u32,
    pub batch_medatada: BatchMetadata,
    pub inclusion_proof: Vec<u8>,
    pub quorum_indexes: Vec<u8>,
}

impl BlobVerificationProof {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend(&self.batch_id.to_be_bytes());
        bytes.extend(&self.blob_index.to_be_bytes());
        bytes.extend(self.batch_medatada.to_bytes());
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

#[derive(Debug, PartialEq, Clone)]
pub struct BlobInfo {
    pub blob_header: BlobHeader,
    pub blob_verification_proof: BlobVerificationProof,
}

impl BlobInfo {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        let blob_header_bytes = self.blob_header.to_bytes();
        bytes.extend(blob_header_bytes.len().to_be_bytes());
        bytes.extend(blob_header_bytes);
        let blob_verification_proof_bytes = self.blob_verification_proof.to_bytes();
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_blob_info_encoding_and_decoding() {
        let blob_info = BlobInfo {
            blob_header: BlobHeader {
                commitment: G1Commitment {
                    x: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                    y: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ],
                },
                data_length: 4,
                blob_quorum_params: vec![
                    BlobQuorumParam {
                        quorum_number: 0,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                    BlobQuorumParam {
                        quorum_number: 1,
                        adversary_threshold_percentage: 33,
                        confirmation_threshold_percentage: 55,
                        chunk_length: 1,
                    },
                ],
            },
            blob_verification_proof: BlobVerificationProof {
                batch_id: 66507,
                blob_index: 92,
                batch_medatada: BatchMetadata {
                    batch_header: BatchHeader {
                        batch_root: vec![
                            179, 187, 53, 98, 192, 80, 151, 28, 125, 192, 115, 29, 129, 238, 216,
                            8, 213, 210, 203, 143, 181, 19, 146, 113, 98, 131, 39, 238, 149, 248,
                            211, 43,
                        ],
                        quorum_numbers: vec![0, 1],
                        quorum_signed_percentages: vec![100, 100],
                        reference_block_number: 2624794,
                    },
                    signatory_record_hash: vec![
                        172, 32, 172, 142, 197, 52, 84, 143, 120, 26, 190, 9, 143, 217, 62, 19, 17,
                        107, 105, 67, 203, 5, 172, 249, 6, 60, 105, 240, 134, 34, 66, 133,
                    ],
                    fee: vec![0],
                    confirmation_block_number: 2624876,
                    batch_header_hash: vec![
                        122, 115, 2, 85, 233, 75, 121, 85, 51, 81, 248, 170, 198, 252, 42, 16, 1,
                        146, 96, 218, 159, 44, 41, 40, 94, 247, 147, 11, 255, 68, 40, 177,
                    ],
                },
                inclusion_proof: vec![
                    203, 160, 237, 48, 117, 255, 75, 254, 117, 144, 164, 77, 29, 146, 36, 48, 190,
                    140, 50, 100, 144, 237, 125, 125, 75, 54, 210, 247, 147, 23, 48, 189, 120, 4,
                    125, 123, 195, 244, 207, 239, 145, 109, 0, 21, 11, 162, 109, 79, 192, 100, 138,
                    157, 203, 22, 17, 114, 234, 72, 174, 231, 209, 133, 99, 118, 201, 160, 137,
                    128, 112, 84, 34, 136, 174, 139, 96, 26, 246, 148, 134, 52, 200, 229, 160, 145,
                    5, 120, 18, 187, 51, 11, 109, 91, 237, 171, 215, 207, 90, 95, 146, 54, 135,
                    166, 66, 157, 255, 237, 69, 183, 141, 45, 162, 145, 71, 16, 87, 184, 120, 84,
                    156, 220, 159, 4, 99, 48, 191, 203, 136, 112, 127, 226, 192, 184, 110, 6, 177,
                    182, 109, 207, 197, 239, 161, 132, 17, 89, 56, 137, 205, 202, 101, 97, 60, 162,
                    253, 23, 169, 75, 236, 211, 126, 121, 132, 191, 68, 167, 200, 16, 154, 149,
                    202, 197, 7, 191, 26, 8, 67, 3, 37, 137, 16, 153, 30, 209, 238, 53, 233, 148,
                    198, 253, 94, 216, 73, 25, 190, 205, 132, 208, 255, 219, 170, 98, 17, 160, 179,
                    183, 200, 17, 99, 36, 130, 216, 223, 72, 222, 250, 73, 78, 79, 72, 253, 105,
                    245, 84, 244, 196,
                ],
                quorum_indexes: vec![0, 1],
            },
        };

        let encoded_blob_info = rlp::encode(&blob_info);
        let decoded_blob_info: BlobInfo = rlp::decode(&encoded_blob_info).unwrap();

        assert_eq!(blob_info, decoded_blob_info);
    }
}

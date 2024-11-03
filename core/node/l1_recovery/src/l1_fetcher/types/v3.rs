use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zkevm_circuits::eip_4844::ethereum_4844_data_into_zksync_pubdata;
use zksync_basic_types::U256;

use super::{
    common::{parse_resolved_pubdata, read_next_n_bytes, ExtractedToken},
    L2ToL1Pubdata, ParseError,
};
use crate::l1_fetcher::{
    blob_http_client::BlobClient,
    constants::zksync::{CALLDATA_SOURCE_TAIL_SIZE, PUBDATA_COMMITMENT_SIZE},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PubdataSource {
    Calldata,
    Blob,
}

impl TryFrom<u8> for PubdataSource {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PubdataSource::Calldata),
            1 => Ok(PubdataSource::Blob),
            _ => Err(ParseError::InvalidPubdataSource(String::from(
                "InvalidPubdataSource",
            ))),
        }
    }
}

/// Data needed to commit new block
#[derive(Debug, Serialize, Deserialize)]
pub struct V3 {
    pub pubdata_source: PubdataSource,
    /// ZKSync batch number
    pub l1_batch_number: u64,
    /// Unix timestamp denoting the start of the block execution.
    pub timestamp: u64,
    /// The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more.
    pub index_repeated_storage_changes: u64,
    /// The state root of the full state tree.
    pub new_state_root: Vec<u8>,
    /// Number of priority operations to be processed.
    pub number_of_l1_txs: U256,
    /// Hash of all priority operations from this block.
    pub priority_operations_hash: Vec<u8>,
    /// Concatenation of all L2 -> L1 system logs in the block.
    pub system_logs: Vec<u8>,
    /// Unparsed blob commitments; must be either parsed, or parsed and resolved using some blob storage server (depending on `pubdata_source`).
    pub pubdata_commitments: Vec<u8>,
}

impl TryFrom<&ethabi::Token> for V3 {
    type Error = ParseError;

    /// Try to parse Ethereum ABI token into [`V3`].
    ///
    /// * `token` - ABI token of `CommitBlockInfo` type on Ethereum.
    fn try_from(token: &ethabi::Token) -> Result<Self, Self::Error> {
        let ExtractedToken {
            l1_batch_number,
            timestamp,
            new_enumeration_index,
            state_root,
            number_of_l1_txs,
            priority_operations_hash,
            system_logs,
            total_l2_to_l1_pubdata,
        } = token.try_into()?;
        let new_enumeration_index = new_enumeration_index.as_u64();

        let mut pointer = 0;
        let pubdata_source = parse_pubdata_source(&total_l2_to_l1_pubdata, &mut pointer)?;
        let pubdata_commitments =
            total_l2_to_l1_pubdata[pointer..total_l2_to_l1_pubdata.len()].to_vec();
        let blk = V3 {
            pubdata_source,
            l1_batch_number: l1_batch_number.as_u64(),
            timestamp: timestamp.as_u64(),
            index_repeated_storage_changes: new_enumeration_index,
            new_state_root: state_root,
            number_of_l1_txs,
            priority_operations_hash,
            system_logs,
            pubdata_commitments,
        };

        Ok(blk)
    }
}

impl V3 {
    pub async fn parse_pubdata(
        &self,
        client: &Arc<dyn BlobClient>,
    ) -> Result<Vec<L2ToL1Pubdata>, ParseError> {
        let bytes = &self.pubdata_commitments[..];
        match self.pubdata_source {
            PubdataSource::Calldata => {
                let l = bytes.len();
                if l < CALLDATA_SOURCE_TAIL_SIZE {
                    Err(ParseError::InvalidCalldata("too short".to_string()))
                } else {
                    parse_resolved_pubdata(&bytes[..l - CALLDATA_SOURCE_TAIL_SIZE])
                }
            }
            PubdataSource::Blob => parse_pubdata_from_blobs(bytes, client).await,
        }
    }
}

// Read the source of the pubdata from a byte array.
fn parse_pubdata_source(bytes: &[u8], pointer: &mut usize) -> Result<PubdataSource, ParseError> {
    let pubdata_source = u8::from_be_bytes(read_next_n_bytes(bytes, pointer));
    pubdata_source.try_into()
}

async fn parse_pubdata_from_blobs(
    bytes: &[u8],
    client: &Arc<dyn BlobClient>,
) -> Result<Vec<L2ToL1Pubdata>, ParseError> {
    let mut pointer = 0;
    let mut l = bytes.len();
    let mut blobs = Vec::new();
    while pointer < l {
        let pubdata_commitment = &bytes[pointer..pointer + PUBDATA_COMMITMENT_SIZE];
        let blob = client.get_blob(&pubdata_commitment[48..96]).await?;
        let mut blob_bytes = ethereum_4844_data_into_zksync_pubdata(&blob);
        blobs.append(&mut blob_bytes);
        pointer += PUBDATA_COMMITMENT_SIZE;
    }

    l = blobs.len();
    while l > 0 && blobs[l - 1] == 0u8 {
        l -= 1;
    }

    let blobs_view = &blobs[..l];
    parse_resolved_pubdata(blobs_view)
}

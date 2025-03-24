/*
    This file contains the utils for the Celestia Tendermint RPC.
    It is used to get the data root inclusion proof for a given height.
*/
#![allow(dead_code)]
use alloy_sol_types::sol;
use reqwest::{Client, Error as ReqwestError};
use serde::Deserialize;
use zksync_basic_types::{
    ethabi::{decode, Contract, Event, ParamType},
    web3::{BlockId, BlockNumber, CallRequest, FilterBuilder, Log},
    H160, H256, U256,
};
use zksync_eth_client::{
    clients::{DynClient, L1},
    EthInterface,
};
use zksync_da_client::types::DAError;
use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};
pub struct TendermintRPCClient {
    url: String,
    client: Client,
}

impl TendermintRPCClient {
    pub fn new(url: String) -> Self {
        TendermintRPCClient { 
            url, 
            client: Client::new(),
        }
    }

    pub async fn get_data_root_inclusion_proof(
        &self,
        height: u64,
        start: u64,
        end: u64,
    ) -> Result<String, ReqwestError> {
        let url = format!("{}/data_root_inclusion_proof", self.url);
        self.client
            .get(url)
            .query(&[("height", height), ("start", start), ("end", end)])
            .send()
            .await
            .expect("Failed to send request")
            .text()
            .await
    }
}

// Get the latest block relayed to Blobstream
pub async fn get_latest_blobstream_relayed_height(
    client: &Box<DynClient<L1>>,
    contract: &Contract,
    contract_address: H160,
) -> Result<U256, DAError> {
    let request = CallRequest {
        to: Some(contract_address),
        data: Some(
            contract
                .function("latestBlock")
                .unwrap()
                .encode_input(&[])
                .unwrap()
                .into(),
        ),
        ..Default::default()
    };
    let block_num = client
        .block_number()
        .await
        .map_err(to_retriable_da_error)?;
    let result = client
        .call_contract_function(request, Some(BlockId::Number(block_num.into())))
        .await
        .map_err(to_retriable_da_error)?
        .0;
    decode(&[ParamType::Uint(256)], &result).unwrap()[0]
        .clone()
        .into_uint()
        .ok_or_else(|| to_non_retriable_da_error("Could not decode block number"))
}

// Search for the BlobStream update event that includes the target height
pub async fn find_block_range(
    client: &Box<DynClient<L1>>,
    target_height: u64,
    latest_block: U256,
    eth_block_num: BlockNumber,
    blobstream_update_event: &Event,
    contract_address: H160,
    num_pages: u64,
    page_size: u64,
) -> Result<Option<(U256, U256, U256)>, Box<dyn std::error::Error>> {
    if target_height >= latest_block.as_u64() {
        return Ok(None);
    }

    let mut page_start = match eth_block_num {
        BlockNumber::Number(num) => num,
        _ => return Err("Invalid block number".into()),
    };

    for multiplier in 1..=num_pages {
        // Limited to 1000 pages
        let page_end = page_start - page_size * multiplier;
        let filter = FilterBuilder::default()
            .from_block(BlockNumber::Number(page_end))
            .to_block(BlockNumber::Number(page_start))
            .address(vec![contract_address])
            .topics(
                Some(vec![blobstream_update_event.signature()]),
                None,
                None,
                None,
            )
            .build();

        let logs = client.logs(&filter).await?;

        if let Some(log) = logs.iter().find(|log| {
            let commitment = DataCommitmentStored::from_log(log);
            commitment.start_block.as_u64() <= target_height
                && commitment.end_block.as_u64() > target_height
        }) {
            let commitment = DataCommitmentStored::from_log(log);
            return Ok(Some((
                commitment.start_block,
                commitment.end_block,
                commitment.proof_nonce,
            )));
        }

        page_start = page_end;
    }

    Err(format!("No matching block range found after {} pages", num_pages).into())
}

// The BlobStream contract event
pub struct DataCommitmentStored {
    pub proof_nonce: U256,
    pub start_block: U256,
    pub end_block: U256,
    pub data_commitment: H256,
}

impl DataCommitmentStored {
    pub fn from_log(log: &Log) -> Self {
        DataCommitmentStored {
            proof_nonce: decode(&[ParamType::Uint(256)], &log.data.0).unwrap()[0]
                .clone()
                .into_uint()
                .unwrap(),
            start_block: U256::from_big_endian(log.topics[1].as_bytes()),
            end_block: U256::from_big_endian(log.topics[2].as_bytes()),
            data_commitment: H256::from_slice(log.topics[3].as_bytes()),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct DataRootInclusionProofResponse {
    pub jsonrpc: String,
    pub id: i64,
    pub result: DataRootInclusionProofResult,
}

#[derive(Debug, serde::Deserialize)]
pub struct DataRootInclusionProofResult {
    pub proof: DataRootInclusionProof,
}

#[derive(Debug, serde::Deserialize)]
pub struct DataRootInclusionProof {
    #[serde(deserialize_with = "deserialize_base64_vec")]
    pub aunts: Vec<Vec<u8>>,
    pub index: String,
    pub leaf_hash: String,
    pub total: String,
}

pub fn deserialize_base64_vec<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    strings
        .into_iter()
        .map(|s| base64::decode(&s).map_err(serde::de::Error::custom))
        .collect()
}

sol! {
    struct DataRootTuple {
        // Celestia block height the data root was included in.
        // Genesis block is height = 0.
        // First queryable block is height = 1.
        uint256 height;
        // Data root.
        bytes32 dataRoot;
    }

    struct AttestationProof {
        // the attestation nonce that commits to the data root tuple.
        uint256 tupleRootNonce;
        // the data root tuple that was committed to.
        DataRootTuple tuple;
        // the binary Merkle proof of the tuple to the commitment.
        BinaryMerkleProof proof;
    }

    struct BinaryMerkleProof {
        // List of side nodes to verify and calculate tree.
        bytes32[] sideNodes;
        // The key of the leaf to verify.
        uint256 key;
        // The number of leaves in the tree
        uint256 numLeaves;
    }

    struct CelestiaZKStackInput {
        AttestationProof attestationProof;
        bytes equivalenceProof;
        bytes publicValues;
    }
}

/*
    This file contains the utils for the Celestia Tendermint RPC.
    It is used to get the data root inclusion proof for a given height.
*/
#![allow(dead_code)]
use hex;
use lazy_static::lazy_static;
use reqwest::{Client, Error as ReqwestError};
use serde::Deserialize;
use zksync_da_client::types::DAError;
use zksync_eth_client::{
    clients::{DynClient, L1},
    EthInterface,
};
use zksync_types::{
    ethabi::{
        decode, Contract, Event, EventParam, FixedBytes, Function, Param, ParamType,
        StateMutability, Token,
    },
    web3::{contract::Tokenize, BlockId, BlockNumber, CallRequest, FilterBuilder, Log},
    H160, H256, U256,
};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

lazy_static! {
    pub static ref BLOBSTREAM_UPDATE_EVENT: Event = Event {
        name: "DataCommitmentStored".to_string(),
        inputs: vec![
            EventParam {
                name: "proofNonce".to_string(),
                kind: ParamType::Uint(256),
                indexed: false
            },
            EventParam {
                name: "startBlock".to_string(),
                kind: ParamType::Uint(64),
                indexed: true
            },
            EventParam {
                name: "endBlock".to_string(),
                kind: ParamType::Uint(64),
                indexed: true
            },
            EventParam {
                name: "dataCommitment".to_string(),
                kind: ParamType::FixedBytes(32),
                indexed: true
            }
        ],
        anonymous: false
    };
    pub static ref BLOBSTREAM_LATEST_BLOCK_FUNCTION: Function = Function {
        name: "latestBlock".to_string(),
        inputs: vec![],
        outputs: vec![Param {
            name: "".to_string(),
            kind: ParamType::Uint(64),
            internal_type: Some("uint64".to_string()),
        }],
        constant: Some(true),
        state_mutability: StateMutability::View,
    };
}

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

        let request = self
            .client
            .get(url.clone())
            .query(&[("height", height), ("start", start), ("end", end)])
            .build()
            .unwrap();
        tracing::debug!("request: {:?}", request.url().as_str());

        self.client
            .get(url)
            .query(&[("height", height), ("start", start), ("end", end)])
            .send()
            .await
            .expect("Failed to send request")
            .text()
            .await
    }

    pub async fn get_data_root(&self, height: u64) -> Result<[u8; 32], ReqwestError> {
        let url = format!("{}/header", self.url);
        let response = self
            .client
            .get(url)
            .query(&[("height", height)])
            .send()
            .await
            .expect("Failed to send request")
            .text()
            .await?;

        // Parse response JSON to extract data root
        let response_json: serde_json::Value =
            serde_json::from_str(&response).expect("Failed to parse response JSON");

        let data_root_hex = response_json["result"]["header"]["data_hash"]
            .as_str()
            .expect("Failed to get data_hash")
            .trim_start_matches("0x");

        let mut data_root = [0u8; 32];
        hex::decode_to_slice(data_root_hex, &mut data_root).expect("Failed to decode hex string");

        Ok(data_root)
    }
}

// Get the latest block relayed to Blobstream
pub async fn get_latest_blobstream_relayed_height(
    client: &Box<DynClient<L1>>,
    contract_address: H160,
) -> Result<U256, DAError> {
    let request = CallRequest {
        to: Some(contract_address),
        data: Some(
            BLOBSTREAM_LATEST_BLOCK_FUNCTION
                .encode_input(&[])
                .unwrap()
                .into(),
        ),
        ..Default::default()
    };
    let block_num = client.block_number().await.map_err(to_retriable_da_error)?;
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
                Some(vec![BLOBSTREAM_UPDATE_EVENT.signature()]),
                None,
                None,
                None,
            )
            .build();

        let logs = client.logs(&filter).await?;

        if let Some(log) = logs.iter().find_map(|log| {
            let commitment = DataCommitmentStored::from_log(log);
            if commitment.start_block.as_u64() <= target_height
                && commitment.end_block.as_u64() > target_height
            {
                Some(commitment)
            } else {
                None
            }
        }) {
            return Ok(Some((log.start_block, log.end_block, log.proof_nonce)));
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

pub struct DataRootTuple {
    pub height: U256,
    // trying ethabi::FixedBytes instead of H256 to match the blobstream contract
    pub data_root: FixedBytes,
}

impl Tokenize for DataRootTuple {
    fn into_tokens(self) -> Vec<Token> {
        vec![Token::Uint(self.height), Token::FixedBytes(self.data_root)]
    }
}

pub struct AttestationProof {
    pub tuple_root_nonce: U256,
    pub tuple: DataRootTuple,
    pub proof: BinaryMerkleProof,
}
impl Tokenize for AttestationProof {
    fn into_tokens(self) -> Vec<Token> {
        vec![
            Token::Uint(self.tuple_root_nonce),
            Token::Tuple(self.tuple.into_tokens()),
            Token::Tuple(self.proof.into_tokens()),
        ]
    }
}

pub struct BinaryMerkleProof {
    // List of side nodes to verify and calculate tree.
    pub side_nodes: Vec<FixedBytes>,
    // The key of the leaf to verify.
    pub key: U256,
    // The number of leaves in the tree
    pub num_leaves: U256,
}

impl Tokenize for BinaryMerkleProof {
    fn into_tokens(self) -> Vec<Token> {
        vec![
            Token::Array(
                self.side_nodes
                    .into_iter()
                    .map(|s| Token::FixedBytes(s))
                    .collect(),
            ),
            Token::Uint(self.key),
            Token::Uint(self.num_leaves),
        ]
    }
}

pub struct CelestiaZKStackInput {
    pub attestation_proof: AttestationProof,
    pub equivalence_proof: Vec<u8>,
    pub public_values: Vec<u8>,
}

impl Tokenize for CelestiaZKStackInput {
    fn into_tokens(self) -> Vec<Token> {
        vec![
            Token::Tuple(self.attestation_proof.into_tokens()),
            Token::Bytes(self.equivalence_proof),
            Token::Bytes(self.public_values),
        ]
    }
}

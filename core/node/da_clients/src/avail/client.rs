use anyhow::anyhow;
use async_trait::async_trait;
use bytes::Bytes;
use jsonrpsee::ws_client::WsClientBuilder;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, sync::Arc};
use subxt_signer::ExposeSecret;
use zksync_config::configs::da_client::avail::{AvailConfig, AvailSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_types::{
    api_key::APIKey,
    ethabi::{self, Token},
    web3::contract::Tokenize,
    H256, U256,
};

use crate::avail::sdk::RawAvailClient;

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug, Clone)]
pub struct AvailClient {
    config: AvailConfig,
    sdk_client: Option<Arc<RawAvailClient>>,
    api_client: Arc<reqwest::Client>,
    gas_relay_api_key: Option<APIKey>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BridgeAPIResponse {
    blob_root: Option<H256>,
    bridge_root: Option<H256>,
    data_root_index: Option<U256>,
    data_root_proof: Option<Vec<H256>>,
    leaf: Option<H256>,
    leaf_index: Option<U256>,
    leaf_proof: Option<Vec<H256>>,
    range_hash: Option<H256>,
    error: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPISubmissionResponse {
    submission_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPIStatusResponse {
    submission: GasRelayAPISubmission,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GasRelayAPISubmission {
    block_hash: Option<H256>,
    extrinsic_index: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct MerkleProofInput {
    // proof of inclusion for the data root
    data_root_proof: Vec<H256>,
    // proof of inclusion of leaf within blob/bridge root
    leaf_proof: Vec<H256>,
    // abi.encodePacked(startBlock, endBlock) of header range commitment on vectorx
    range_hash: H256,
    // index of the data root in the commitment tree
    data_root_index: U256,
    // blob root to check proof against, or reconstruct the data root
    blob_root: H256,
    // bridge root to check proof against, or reconstruct the data root
    bridge_root: H256,
    // leaf being proven
    leaf: H256,
    // index of the leaf in the blob/bridge root tree
    leaf_index: U256,
}

impl Tokenize for MerkleProofInput {
    fn into_tokens(self) -> Vec<Token> {
        vec![Token::Tuple(vec![
            Token::Array(
                self.data_root_proof
                    .iter()
                    .map(|x| Token::FixedBytes(x.as_bytes().to_vec()))
                    .collect(),
            ),
            Token::Array(
                self.leaf_proof
                    .iter()
                    .map(|x| Token::FixedBytes(x.as_bytes().to_vec()))
                    .collect(),
            ),
            Token::FixedBytes(self.range_hash.as_bytes().to_vec()),
            Token::Uint(self.data_root_index),
            Token::FixedBytes(self.blob_root.as_bytes().to_vec()),
            Token::FixedBytes(self.bridge_root.as_bytes().to_vec()),
            Token::FixedBytes(self.leaf.as_bytes().to_vec()),
            Token::Uint(self.leaf_index),
        ])]
    }
}

impl AvailClient {
    pub async fn new(config: AvailConfig, secrets: AvailSecrets) -> anyhow::Result<Self> {
        let api_client = reqwest::Client::new();
        if config.gas_relay_mode {
            return Ok(Self {
                config,
                sdk_client: None,
                api_client: Arc::new(api_client),
                gas_relay_api_key: secrets.gas_relay_api_key,
            });
        }

        let seed_phrase = secrets
            .seed_phrase
            .ok_or_else(|| anyhow::anyhow!("seed phrase"))?;
        // these unwraps are safe because we validate in protobuf config
        let sdk_client =
            RawAvailClient::new(config.app_id.unwrap(), seed_phrase.0.expose_secret()).await?;

        Ok(Self {
            config,
            sdk_client: Some(Arc::new(sdk_client)),
            api_client: Arc::new(api_client),
            gas_relay_api_key: None,
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for AvailClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch_number
        data: Vec<u8>,
    ) -> anyhow::Result<DispatchResponse, DAError> {
        if self.config.gas_relay_mode {
            let submit_url = format!(
                "{}/user/submit_raw_data?token=ethereum",
                self.config.gas_relay_api_url.clone().unwrap()
            );
            // send the data to the gas relay
            let submit_response = self
                .api_client
                .post(&submit_url)
                .body(Bytes::from(data))
                .header("Content-Type", "text/plain")
                .header(
                    "Authorization",
                    self.gas_relay_api_key
                        .as_ref()
                        .expect("No gas relay api key")
                        .0
                        .expose_secret()
                        .clone(),
                )
                .send()
                .await
                .map_err(to_retriable_da_error)?;
            let submit_response_text = submit_response
                .text()
                .await
                .map_err(to_retriable_da_error)?;
            let submit_response_struct: GasRelayAPISubmissionResponse =
                serde_json::from_str(&submit_response_text.clone())
                    .map_err(to_retriable_da_error)?;
            let status_url = format!(
                "{}/user/get_submission_info?submission_id={}",
                self.config.gas_relay_api_url.clone().unwrap(),
                submit_response_struct.submission_id
            );
            let mut retries = 0;
            let mut status_response: reqwest::Response;
            let mut status_response_text: String;
            let mut status_response_struct: GasRelayAPIStatusResponse;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(u64::try_from(40).unwrap()))
                    .await; // usually takes 20s to finalize
                status_response = self
                    .api_client
                    .get(&status_url)
                    .header(
                        "Authorization",
                        &self
                            .gas_relay_api_key
                            .as_ref()
                            .expect("No gas relay api key")
                            .0
                            .expose_secret()
                            .clone(),
                    )
                    .send()
                    .await
                    .map_err(to_retriable_da_error)?;
                status_response_text = status_response
                    .text()
                    .await
                    .map_err(to_retriable_da_error)?;
                status_response_struct =
                    serde_json::from_str(&status_response_text).map_err(to_retriable_da_error)?;
                if status_response_struct.submission.block_hash.is_some() {
                    break;
                }
                retries += 1;
                if retries > self.config.max_retries {
                    return Err(to_retriable_da_error(anyhow!(
                        "Failed to get gas relay status"
                    )));
                }
            }
            return Ok(DispatchResponse {
                blob_id: format!(
                    "{:x}:{}",
                    status_response_struct.submission.block_hash.unwrap(),
                    status_response_struct.submission.extrinsic_index.unwrap()
                ),
            });
        }
        let client = WsClientBuilder::default()
            .build(self.config.api_node_url.clone().unwrap().as_str())
            .await
            .map_err(to_non_retriable_da_error)?;

        let extrinsic = self
            .sdk_client
            .as_ref()
            .unwrap()
            .build_extrinsic(&client, data)
            .await
            .map_err(to_non_retriable_da_error)?;

        let block_hash = self
            .sdk_client
            .as_ref()
            .unwrap()
            .submit_extrinsic(&client, extrinsic.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;
        let tx_id = self
            .sdk_client
            .as_ref()
            .unwrap()
            .get_tx_id(&client, block_hash.as_str(), extrinsic.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;
        Ok(DispatchResponse::from(format!("{}:{}", block_hash, tx_id)))
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<InclusionData>, DAError> {
        let (block_hash, tx_idx) = blob_id.split_once(':').ok_or_else(|| DAError {
            error: anyhow!("Invalid blob ID format"),
            is_retriable: false,
        })?;

        let url = format!(
            "{}/eth/proof/{}?index={}",
            self.config.bridge_api_url, block_hash, tx_idx
        );

        let response = self
            .api_client
            .get(&url)
            .send()
            .await
            .map_err(to_retriable_da_error)?;
        let bridge_api_data = response
            .json::<BridgeAPIResponse>()
            .await
            .map_err(to_retriable_da_error)?;
        if bridge_api_data.error.is_some() {
            return Err(to_retriable_da_error(anyhow!(format!(
                "Bridge API returned error: {}",
                bridge_api_data.error.unwrap()
            ))));
        }

        let attestation_data: MerkleProofInput = MerkleProofInput {
            data_root_proof: bridge_api_data.data_root_proof.unwrap(),
            leaf_proof: bridge_api_data.leaf_proof.unwrap(),
            range_hash: bridge_api_data.range_hash.unwrap(),
            data_root_index: bridge_api_data.data_root_index.unwrap(),
            blob_root: bridge_api_data.blob_root.unwrap(),
            bridge_root: bridge_api_data.bridge_root.unwrap(),
            leaf: bridge_api_data.leaf.unwrap(),
            leaf_index: bridge_api_data.leaf_index.unwrap(),
        };
        Ok(Some(InclusionData {
            data: ethabi::encode(&attestation_data.into_tokens()),
        }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(RawAvailClient::MAX_BLOB_SIZE)
    }
}

pub fn to_non_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: false,
    }
}

pub fn to_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: true,
    }
}

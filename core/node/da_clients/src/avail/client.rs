use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use http::StatusCode;
use jsonrpsee::ws_client::WsClientBuilder;
use reqwest::Url;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use zksync_config::configs::da_client::avail::{AvailClientConfig, AvailConfig};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_types::{
    ethabi::{self, Token},
    web3::contract::Tokenize,
    H256, U256,
};

use crate::{
    avail::sdk::{GasRelayClient, RawAvailClient},
    utils::{to_non_retriable_da_error, to_retriable_da_error},
};

#[derive(Debug, Clone)]
enum AvailClientMode {
    Default(Box<RawAvailClient>),
    GasRelay(GasRelayClient),
}

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug, Clone)]
pub struct AvailClient {
    config: AvailConfig,
    sdk_client: Arc<AvailClientMode>,
    api_client: Arc<reqwest::Client>, // bridge API reqwest client
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BridgeAPIResponse {
    blob_root: Option<H256>,
    bridge_root: Option<H256>,
    #[serde(deserialize_with = "deserialize_u256_from_integer")]
    data_root_index: Option<U256>,
    data_root_proof: Option<Vec<H256>>,
    leaf: Option<H256>,
    #[serde(deserialize_with = "deserialize_u256_from_integer")]
    leaf_index: Option<U256>,
    leaf_proof: Option<Vec<H256>>,
    range_hash: Option<H256>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum U256Value {
    Number(u64),
    String(String),
}

fn deserialize_u256_from_integer<'de, D>(deserializer: D) -> Result<Option<U256>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    match Option::<U256Value>::deserialize(deserializer)? {
        Some(U256Value::Number(num)) => Ok(Some(U256::from(num))),
        Some(U256Value::String(s)) => U256::from_str_radix(s.strip_prefix("0x").unwrap_or(&s), 16)
            .map(Some)
            .map_err(|e| D::Error::custom(format!("failed to parse hex string: {}", e))),
        None => Ok(None),
    }
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
    pub async fn new(config: AvailConfig) -> anyhow::Result<Self> {
        let api_client = Arc::new(reqwest::Client::new());
        match config.client.clone() {
            AvailClientConfig::GasRelay(conf) => {
                let gas_relay_api_key = conf.gas_relay_api_key;
                let gas_relay_client = GasRelayClient::new(
                    &conf.gas_relay_api_url,
                    gas_relay_api_key.0.expose_secret(),
                    conf.max_retries,
                    Arc::clone(&api_client),
                )
                .await?;
                Ok(Self {
                    config,
                    sdk_client: Arc::new(AvailClientMode::GasRelay(gas_relay_client)),
                    api_client,
                })
            }
            AvailClientConfig::FullClient(conf) => {
                let seed_phrase = conf.seed_phrase;
                let sdk_client = RawAvailClient::new(
                    conf.app_id,
                    seed_phrase.0.expose_secret(),
                    conf.max_blocks_to_look_back,
                )
                .await?;

                Ok(Self {
                    config,
                    sdk_client: Arc::new(AvailClientMode::Default(Box::new(sdk_client))),
                    api_client,
                })
            }
        }
    }
}

#[async_trait]
impl DataAvailabilityClient for AvailClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch_number
        data: Vec<u8>,
    ) -> anyhow::Result<DispatchResponse, DAError> {
        match self.sdk_client.as_ref() {
            AvailClientMode::Default(client) => {
                let default_config = match &self.config.client {
                    AvailClientConfig::FullClient(conf) => conf,
                    _ => unreachable!(), // validated in protobuf config
                };
                let ws_client = WsClientBuilder::default()
                    .build(default_config.api_node_url.clone().as_str())
                    .await
                    .map_err(to_non_retriable_da_error)?;

                let extrinsic = client
                    .build_extrinsic(&ws_client, data)
                    .await
                    .map_err(to_non_retriable_da_error)?;

                let extrinsic_hash = tokio::time::timeout(
                    default_config.dispatch_timeout,
                    client.submit_extrinsic(&ws_client, extrinsic.as_str()),
                )
                .await
                .map_err(|_| DAError {
                    error: anyhow!("Timeout while submitting extrinsic"),
                    is_retriable: true,
                })?
                .map_err(to_retriable_da_error)?;

                Ok(DispatchResponse::from(extrinsic_hash))
            }
            AvailClientMode::GasRelay(client) => {
                let submission_id = client
                    .post_data(data)
                    .await
                    .map_err(to_retriable_da_error)?;
                Ok(DispatchResponse {
                    request_id: submission_id,
                })
            }
        }
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        dispatched_at: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        Ok(match self.sdk_client.as_ref() {
            AvailClientMode::Default(client) => {
                let default_config = match &self.config.client {
                    AvailClientConfig::FullClient(conf) => conf,
                    _ => unreachable!(), // validated in protobuf config
                };

                if Utc::now()
                    .signed_duration_since(dispatched_at)
                    .to_std()
                    .map_err(to_retriable_da_error)?
                    > default_config.dispatch_timeout
                {
                    return Err(DAError {
                        error: anyhow!("Dispatch timeout exceeded"),
                        is_retriable: false,
                    });
                }

                let ws_client = WsClientBuilder::default()
                    .build(default_config.api_node_url.clone().as_str())
                    .await
                    .map_err(to_non_retriable_da_error)?;

                client
                    .search_for_ext_in_latest_block(&ws_client, &dispatch_request_id)
                    .await
                    .map_err(to_retriable_da_error)?
                    .map(|(block_hash, tx_id)| FinalityResponse {
                        blob_id: format!("{}:{}", block_hash, tx_id),
                    })
            }
            AvailClientMode::GasRelay(client) => {
                let Some((block_hash, extrinsic_index)) = client
                    .check_finality(dispatch_request_id)
                    .await
                    .map_err(to_retriable_da_error)?
                else {
                    return Ok(None);
                };

                Some(FinalityResponse {
                    blob_id: format!("{:x}:{}", block_hash, extrinsic_index),
                })
            }
        })
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<InclusionData>, DAError> {
        let (block_hash, tx_idx) = blob_id.split_once(':').ok_or_else(|| DAError {
            error: anyhow!("Invalid blob ID format"),
            is_retriable: false,
        })?;
        let url = Url::parse(&self.config.bridge_api_url)
            .map_err(|_| DAError {
                error: anyhow!("Invalid URL"),
                is_retriable: false,
            })?
            .join(format!("/eth/proof/{}?index={}", block_hash, tx_idx).as_str())
            .map_err(|_| DAError {
                error: anyhow!("Unable to join to URL"),
                is_retriable: false,
            })?;

        let response = self
            .api_client
            .get(url)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(to_retriable_da_error)?;

        // 404 means that the blob is not included in the bridge yet
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let bridge_api_data = response
            .json::<BridgeAPIResponse>()
            .await
            .map_err(to_retriable_da_error)?;

        tracing::info!("Bridge API Response: {:?}", bridge_api_data);

        // Check if there's an error in the response
        if let Some(err) = bridge_api_data.error {
            tracing::info!(
                "Bridge API returned error: {:?}. Data might not be available yet.",
                err
            );
            return Ok(None);
        }

        match bridge_response_to_merkle_proof_input(bridge_api_data) {
            Some(attestation_data) => Ok(Some(InclusionData {
                data: ethabi::encode(&attestation_data.into_tokens()),
            })),
            None => {
                tracing::info!(
                    "Bridge API response missing required fields. Data might not be available yet."
                );
                Ok(None)
            }
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(RawAvailClient::MAX_BLOB_SIZE)
    }

    fn client_type(&self) -> ClientType {
        ClientType::Avail
    }

    async fn balance(&self) -> Result<u64, DAError> {
        match self.sdk_client.as_ref() {
            AvailClientMode::Default(client) => {
                let AvailClientConfig::FullClient(default_config) = &self.config.client else {
                    unreachable!(); // validated in protobuf config
                };

                let ws_client = WsClientBuilder::default()
                    .build(default_config.api_node_url.clone().as_str())
                    .await
                    .map_err(to_non_retriable_da_error)?;

                Ok(client
                    .balance(&ws_client)
                    .await
                    .map_err(to_non_retriable_da_error)?)
            }
            AvailClientMode::GasRelay(_) => {
                Ok(0) // TODO: implement balance for gas relay (PE-304)
            }
        }
    }
}

fn bridge_response_to_merkle_proof_input(
    bridge_api_response: BridgeAPIResponse,
) -> Option<MerkleProofInput> {
    Some(MerkleProofInput {
        data_root_proof: bridge_api_response.data_root_proof?,
        leaf_proof: bridge_api_response.leaf_proof?,
        range_hash: bridge_api_response.range_hash?,
        data_root_index: bridge_api_response.data_root_index?,
        blob_root: bridge_api_response.blob_root?,
        bridge_root: bridge_api_response.bridge_root?,
        leaf: bridge_api_response.leaf?,
        leaf_index: bridge_api_response.leaf_index?,
    })
}

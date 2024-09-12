use anyhow::anyhow;
use async_trait::async_trait;
use std::{fmt::Debug, sync::Arc};

use near_jsonrpc_client::methods::{
    block::RpcBlockRequest,
    light_client_proof::{
        RpcLightClientExecutionProofRequest, RpcLightClientExecutionProofResponse,
    },
};
use near_primitives::{
    block_header::BlockHeader,
    borsh,
    hash::CryptoHash,
    merkle::compute_root_from_path,
    types::{AccountId, TransactionOrReceiptId},
    views::LightClientBlockLiteView,
};

use zksync_da_client::{
    types::{self, DAError},
    DataAvailabilityClient,
};

use crate::{
    near::{
        sdk::{DAClient, DataAvailability},
        types::{BlobInclusionProof, LatestHeaderResponse},
    },
    utils::to_non_retriable_da_error,
};

use zksync_config::NearConfig;

#[derive(Clone)]
pub struct NearClient {
    config: NearConfig,
    da_rpc_client: Arc<DAClient>,
}

#[async_trait]
trait LightClient {
    async fn latest_header(&self) -> Result<CryptoHash, types::DAError>;
    async fn get_header(
        &self,
        latest_header: CryptoHash,
    ) -> Result<LightClientBlockLiteView, types::DAError>;
    async fn get_proof(
        &self,
        transaction_hash: &str,
        latest_header: CryptoHash,
    ) -> Result<RpcLightClientExecutionProofResponse, types::DAError>;
}

#[async_trait]
impl LightClient for NearClient {
    async fn latest_header(&self) -> Result<CryptoHash, types::DAError> {
        let client = reqwest::Client::new();
        let selector = "0xbba3fe8e"; // Selector for `latestHeader` method

        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": &self.config.bridge_contract,  // Target contract address
                "data": &selector // Selector + Encoded address
            }, "latest"],
            "id": 1
        });

        // Send the request
        let response = client
            .post(&self.config.evm_provider_url) // Replace with your Ethereum node URL
            .json(&rpc_payload)
            .send()
            .await
            .map_err(|e| DAError {
                error: anyhow!(e),
                is_retriable: true,
            })?;

        let resp = response.text().await.map_err(to_non_retriable_da_error)?;

        // Deserialize the JSON response into the JsonResponse struct
        serde_json::from_str::<LatestHeaderResponse>(&resp)
            .map(|resp| resp.result)
            .map(|hash| hex_to_bytes(&hash))
            .map_err(to_non_retriable_da_error)?
            .map(CryptoHash)
    }

    async fn get_header(
        &self,
        latest_header: CryptoHash,
    ) -> Result<LightClientBlockLiteView, types::DAError> {
        let req = RpcBlockRequest {
            block_reference: near_primitives::types::BlockReference::BlockId(
                near_primitives::types::BlockId::Hash(latest_header),
            ),
        };

        let block_light_view: LightClientBlockLiteView = self
            .da_rpc_client
            .client
            .call(req)
            .await
            .map_err(|e| DAError {
                error: anyhow!(e),
                is_retriable: true,
            })
            .map(|x| x.header)
            .map(BlockHeader::from)
            .map(Into::into)?;

        Ok(block_light_view)
    }

    async fn get_proof(
        &self,
        transaction_hash: &str,
        latest_header: CryptoHash,
    ) -> Result<RpcLightClientExecutionProofResponse, types::DAError> {
        let req = RpcLightClientExecutionProofRequest {
            id: TransactionOrReceiptId::Transaction {
                transaction_hash: transaction_hash
                    .parse::<CryptoHash>()
                    .map_err(|e| to_non_retriable_da_error(anyhow!(e)))?,
                sender_id: self
                    .config
                    .account_id
                    .parse::<AccountId>()
                    .map_err(to_non_retriable_da_error)?,
            },
            light_client_head: latest_header,
        };

        let proof = self
            .da_rpc_client
            .client
            .call(req)
            .await
            .map_err(|e| DAError {
                error: anyhow!(e),
                is_retriable: true,
            })?;

        Ok(proof)
    }
}

impl Debug for NearClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NearClient")
            .field("config", &self.config)
            .field("client", &self.da_rpc_client)
            .finish()
    }
}

impl NearClient {
    pub async fn new(config: NearConfig) -> anyhow::Result<Self> {
        let da_rpc_client = DAClient::new(&config)
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(Self {
            config,
            da_rpc_client: Arc::new(da_rpc_client),
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for NearClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let blob_hash = self.da_rpc_client.submit(data).await?;

        let blob_id = blob_hash.to_string();

        Ok(types::DispatchResponse { blob_id })
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> Result<Option<types::InclusionData>, types::DAError> {
        // Call bridge_contract `latestHeader` method to get the latest ZK-verified block header hash
        let latest_header = self.latest_header().await?;
        let latest_header_view = self.get_header(latest_header).await?;
        let latest_header_hash = latest_header_view.hash();

        if latest_header_hash != latest_header {
            return Err(DAError {
                error: anyhow!("Light client header mismatch"),
                is_retriable: false,
            });
        }

        let proof = self.get_proof(blob_id, latest_header).await?;
        let head_block_root = latest_header_view.inner_lite.block_merkle_root;

        verify_proof(head_block_root, &proof)?;

        let attestation_data = BlobInclusionProof {
            outcome_proof: proof
                .outcome_proof
                .try_into()
                .map_err(to_non_retriable_da_error)?,
            outcome_root_proof: proof.outcome_root_proof,
            block_header_lite: proof
                .block_header_lite
                .try_into()
                .map_err(to_non_retriable_da_error)?,
            block_proof: proof.block_proof,
            head_merkle_root: head_block_root.0,
        };

        Ok(Some(types::InclusionData {
            data: borsh::to_vec(&attestation_data).map_err(to_non_retriable_da_error)?,
        }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> std::option::Option<usize> {
        Some(1572864)
    }
}

fn hex_to_bytes(hex: &str) -> Result<[u8; 32], DAError> {
    let hex = hex.trim_start_matches("0x");

    // Ensure the hex string represents exactly 32 bytes (64 hex characters)
    if hex.len() != 64 {
        return Err(DAError {
            error: anyhow!("Hex string must represent exactly 32 bytes (64 hex digits)"),
            is_retriable: false,
        });
    }

    let mut bytes = [0u8; 32];

    // Iterate over the hex string in steps of 2 and convert each pair to a byte
    for i in 0..32 {
        let byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| DAError {
            error: anyhow!("Invalid hex character at position {}", i * 2),
            is_retriable: false,
        })?;
        bytes[i] = byte;
    }

    Ok(bytes)
}

fn verify_proof(
    head_block_root: CryptoHash,
    proof: &RpcLightClientExecutionProofResponse,
) -> Result<(), DAError> {
    let expected_outcome_root = proof.block_header_lite.inner_lite.outcome_root;

    let outcome_hash = CryptoHash::hash_borsh(proof.outcome_proof.to_hashes());
    let outcome_root = compute_root_from_path(&proof.outcome_proof.proof, outcome_hash);
    let leaf = CryptoHash::hash_borsh(outcome_root);
    let outcome_root = compute_root_from_path(&proof.outcome_root_proof, leaf);

    if expected_outcome_root != outcome_root {
        return Err(DAError {
            error: anyhow!("Calculated outcome_root does not match proof.block_header_lite.inner_lite.outcome_root"),
            is_retriable: false,
        });
    }

    // Verify proof block root matches the light client head block root
    let block_hash = proof.block_header_lite.hash();
    let block_root = compute_root_from_path(&proof.block_proof, block_hash);
    if head_block_root != block_root {
        return Err(DAError {
            error: anyhow!("Calculated block_merkle_root does not match head_block_root"),
            is_retriable: false,
        });
    }

    Ok(())
}

use std::str::FromStr;

use anyhow::anyhow;
use near_crypto::{InMemorySigner, SecretKey, Signer};
use near_jsonrpc_client::{
    methods::{block::RpcBlockRequest, query::RpcQueryRequest, send_tx::RpcSendTransactionRequest},
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::{
    light_client::{RpcLightClientExecutionProofRequest, RpcLightClientExecutionProofResponse},
    query::QueryResponseKind,
};
use near_primitives::{
    block::BlockHeader,
    borsh,
    hash::CryptoHash,
    merkle::compute_root_from_path,
    transaction::{Action, FunctionCallAction, Transaction, TransactionV0},
    types::{AccountId, BlockReference, TransactionOrReceiptId},
    views::{
        FinalExecutionOutcomeViewEnum, FinalExecutionStatus, LightClientBlockLiteView,
        TxExecutionStatus,
    },
};
use zksync_config::NearConfig;
use zksync_da_client::types::{self, DAError};

use super::{types::LatestHeaderResponse, NearClient};
use crate::utils::to_non_retriable_da_error;

const GAS_LIMIT: u64 = 20_000_000_000_000;

#[derive(Debug)]
pub(crate) struct DAClient {
    pub(crate) client: JsonRpcClient,
    contract: AccountId,
    account_id: AccountId,
    secret_key: SecretKey,
}

impl DAClient {
    pub(crate) async fn new(config: &NearConfig) -> anyhow::Result<Self> {
        let client = JsonRpcClient::connect(&config.rpc_client_url);
        let contract: AccountId = config.blob_contract.parse()?;
        let account_id: AccountId = config.account_id.parse()?;
        let secret_key = SecretKey::from_str(&config.secret_key)?;
        Ok(Self {
            client,
            contract,
            account_id,
            secret_key,
        })
    }

    async fn get_block_hash_and_nonce(
        &self,
        account_id: &AccountId,
        public_key: &near_crypto::PublicKey,
    ) -> Result<(CryptoHash, u64), DAError> {
        let query_req = RpcQueryRequest {
            block_reference: BlockReference::latest(),
            request: near_primitives::views::QueryRequest::ViewAccessKey {
                account_id: account_id.clone(),
                public_key: public_key.clone(),
            },
        };

        let query_resp = self
            .client
            .call(query_req)
            .await
            .map_err(to_non_retriable_da_error)?;

        match query_resp.kind {
            QueryResponseKind::AccessKey(access_key) => {
                Ok((query_resp.block_hash, access_key.nonce))
            }
            _ => Err(DAError {
                error: anyhow::anyhow!("Invalid response"),
                is_retriable: false,
            }),
        }
    }

    pub(crate) async fn submit(&self, blob: Vec<u8>) -> Result<CryptoHash, DAError> {
        let action = FunctionCallAction {
            method_name: "submit".to_string(),
            args: borsh::to_vec(&blob).map_err(to_non_retriable_da_error)?,
            gas: GAS_LIMIT,
            deposit: 0,
        };

        let signer =
            InMemorySigner::from_secret_key(self.account_id.clone(), self.secret_key.clone());

        let (block_hash, nonce) = self
            .get_block_hash_and_nonce(&signer.account_id, &signer.public_key)
            .await?;

        let tx = Transaction::V0(TransactionV0 {
            signer_id: signer.account_id.clone(),
            public_key: signer.public_key.clone(),
            nonce: nonce + 1,
            receiver_id: self.contract.clone(),
            block_hash,
            actions: vec![Action::FunctionCall(Box::new(action))],
        });

        let tx_req = RpcSendTransactionRequest {
            signed_transaction: tx.sign(&Signer::InMemory(signer)),
            wait_until: TxExecutionStatus::Final,
        };

        match self
            .client
            .call(&tx_req)
            .await
            .map_err(to_non_retriable_da_error)?
            .final_execution_outcome
            .map(FinalExecutionOutcomeViewEnum::into_outcome)
        {
            Some(outcome) => match outcome.status {
                FinalExecutionStatus::SuccessValue(_) => Ok(outcome.transaction.hash),
                FinalExecutionStatus::Failure(e) => Err(DAError {
                    error: anyhow::anyhow!("Error submitting transaction: {:?}", e),
                    is_retriable: false,
                }),
                _ => Err(DAError {
                    error: anyhow::anyhow!(
                        "Transaction not ready yet, this should not be reachable"
                    ),
                    is_retriable: false,
                }),
            },
            None => Err(DAError {
                error: anyhow::anyhow!("Transaction not ready yet"),
                is_retriable: true,
            }),
        }
    }

    pub(crate) async fn latest_header(
        &self,
        config: &NearConfig,
    ) -> Result<CryptoHash, types::DAError> {
        let client = reqwest::Client::new();
        let selector = "0xbba3fe8e"; // Selector for `latestHeader` method

        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": &config.bridge_contract,  // Target contract address
                "data": &selector // Selector + Encoded address
            }, "latest"],
            "id": 1
        });

        // Send the request
        let response = client
            .post(&config.evm_provider_url) // Replace with your Ethereum node URL
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

    pub(crate) async fn get_header(
        &self,
        latest_header: CryptoHash,
    ) -> Result<LightClientBlockLiteView, types::DAError> {
        let req = RpcBlockRequest {
            block_reference: near_primitives::types::BlockReference::BlockId(
                near_primitives::types::BlockId::Hash(latest_header),
            ),
        };

        let block_light_view: LightClientBlockLiteView = self
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

    pub(crate) async fn get_proof(
        &self,
        transaction_hash: &str,
        latest_header: CryptoHash,
    ) -> Result<RpcLightClientExecutionProofResponse, types::DAError> {
        let req = RpcLightClientExecutionProofRequest {
            id: TransactionOrReceiptId::Transaction {
                transaction_hash: transaction_hash
                    .parse::<CryptoHash>()
                    .map_err(|e| to_non_retriable_da_error(anyhow!(e)))?,
                sender_id: self.account_id.clone(),
            },
            light_client_head: latest_header,
        };

        let proof = self.client.call(req).await.map_err(|e| DAError {
            error: anyhow!(e),
            is_retriable: true,
        })?;

        Ok(proof)
    }
}

pub(crate) fn hex_to_bytes(hex: &str) -> Result<[u8; 32], DAError> {
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

pub(crate) fn verify_proof(
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

use std::str::FromStr;
use zksync_config::NearConfig;

use near_crypto::{InMemorySigner, SecretKey, Signer};
use near_jsonrpc_client::{
    methods::{query::RpcQueryRequest, send_tx::RpcSendTransactionRequest},
    JsonRpcClient,
};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{
    hash::CryptoHash,
    transaction::{Action, FunctionCallAction, Transaction, TransactionV0},
    types::{AccountId, BlockReference},
    views::{FinalExecutionOutcomeViewEnum, FinalExecutionStatus, TxExecutionStatus},
};

use crate::utils::to_non_retriable_da_error;
use zksync_da_client::types::DAError;

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
        let client = JsonRpcClient::connect(&config.da_rpc_url);
        let contract: AccountId = config.contract.parse()?;
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
}

#[async_trait::async_trait]
pub trait DataAvailability {
    /// Submit blobs to the da layer
    async fn submit(&self, blob: Vec<u8>) -> Result<CryptoHash, DAError>;
}

#[async_trait::async_trait]
impl DataAvailability for DAClient {
    async fn submit(&self, blob: Vec<u8>) -> Result<CryptoHash, DAError> {
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
}

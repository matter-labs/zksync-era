use async_trait::async_trait;
// use serde_json::Value;
use ethers::{
    providers::{JsonRpcClient, Provider, ProviderError},
    types::Bytes,
    types::{Address, TransactionReceipt, TxHash, H160, H256, U64},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zksync_system_constants::L1_MESSENGER_ADDRESS;
use zksync_types::ethabi;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Proof {
    pub id: u64,
    #[serde(rename = "proof")]
    pub merkle_proof: Vec<H256>,
    pub root: Bytes,
}

/// Represents a L2 to L1 transaction log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2ToL1Log {
    /// The block number.
    #[serde(rename = "blockNumber")]
    pub block_number: U64,
    /// The block hash.
    #[serde(rename = "blockHash")]
    pub block_hash: H256,
    /// The batch number on L1.
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: U64,
    /// The L2 transaction number in a block, in which the log was sent.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: U64,
    /// The transaction log index.
    #[serde(rename = "transactionLogIndex")]
    pub transaction_log_index: U64,
    /// The transaction index in L1 batch.
    #[serde(rename = "txIndexInL1Batch")]
    pub tx_index_in_l1_batch: Option<U64>,
    /// The shard identifier, 0 - rollup, 1 - porter.
    #[serde(rename = "shardId")]
    pub shard_id: U64,
    /// A boolean flag that is part of the log along with `key`, `value`, and `sender` address.
    /// This field is required formally but does not have any special meaning.
    #[serde(rename = "isService")]
    pub is_service: bool,
    /// The L2 address which sent the log.
    #[serde(rename = "sender")]
    pub sender: Address,
    /// The 32 bytes of information that was sent in the log.
    #[serde(rename = "key")]
    pub key: H256,
    /// The 32 bytes of information that was sent in the log.
    #[serde(rename = "value")]
    pub value: H256,
    /// The transaction hash.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: H256,
    /// The log index.
    #[serde(rename = "logIndex")]
    pub log_index: U64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeWithdrawalParams {
    pub l2_batch_number: U64,
    pub l2_message_index: U64,
    pub l2_tx_number_in_block: U64,
    pub message: Bytes,
    pub sender: Address,
    pub proof: Proof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKTransactionReceipt {
    #[serde(flatten)]
    pub base: TransactionReceipt,
    /// The batch number on the L1 network.
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: Option<U64>,
    /// The transaction index within the batch on the L1 network.
    #[serde(rename = "l1BatchTxIndex")]
    pub l1_batch_tx_index: Option<U64>,
    /// The logs of L2 to L1 messages.
    #[serde(rename = "l2ToL1Logs")]
    pub l2_to_l1_logs: Vec<L2ToL1Log>,
}

/// A `Log` is an extension of `ethers::types::Log` with additional features for interacting with ZKsync Era.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKLog {
    #[serde(flatten)]
    pub base: ethers::types::Log,
    /// The batch number on L1.
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: Option<U64>,
}

impl std::ops::Deref for ZKLog {
    type Target = ethers::types::Log;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl std::ops::DerefMut for ZKLog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl ZKTransactionReceipt {
    pub fn new(
        base: TransactionReceipt,
        l1_batch_number: Option<U64>,
        l1_batch_tx_index: Option<U64>,
        l2_to_l1_logs: Vec<L2ToL1Log>,
    ) -> Self {
        Self {
            base,
            l1_batch_number,
            l1_batch_tx_index,
            l2_to_l1_logs,
        }
    }
}

impl std::ops::Deref for ZKTransactionReceipt {
    type Target = TransactionReceipt;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl std::ops::DerefMut for ZKTransactionReceipt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

#[async_trait]
pub trait ZKSProvider {
    type Provider: JsonRpcClient;
    type ZKProvider: JsonRpcClient;

    async fn get_withdrawal_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(ZKLog, U64), ProviderError>;

    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        l2_to_l1_log_index: Option<u64>,
    ) -> Result<Option<Proof>, ProviderError>;

    async fn get_withdrawal_l2_to_l1_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(u64, L2ToL1Log), ProviderError>;

    async fn get_finalize_withdrawal_params(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<FinalizeWithdrawalParams, ProviderError>;

    async fn get_transaction_receipt<T: Send + Sync + Into<TxHash>>(
        &self,
        transaction_hash: T,
    ) -> Result<Option<ZKTransactionReceipt>, ProviderError>;
}

#[async_trait]
impl<P: JsonRpcClient> ZKSProvider for Provider<P> {
    type Provider = P;
    type ZKProvider = P;

    async fn get_withdrawal_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(ZKLog, U64), ProviderError> {
        let receipt = ZKSProvider::get_transaction_receipt(self, withdrawal_hash).await?;

        let receipt = receipt
            .ok_or_else(|| ProviderError::CustomError("Transaction is not mined!".into()))?;

        let l1_message_event_signature: H256 = ethabi::long_signature(
            "L1MessageSent",
            &[
                ethabi::ParamType::Address,
                ethabi::ParamType::FixedBytes(32),
                ethabi::ParamType::Bytes,
            ],
        );

        let log = receipt
            .logs
            .clone()
            .into_iter()
            .filter(|log| {
                log.address == L1_MESSENGER_ADDRESS && log.topics[0] == l1_message_event_signature
            })
            .nth(index)
            .ok_or_else(|| ProviderError::CustomError("Log not found".into()))?;

        Ok((
            ZKLog {
                base: log,
                l1_batch_number: receipt.l1_batch_number,
            },
            receipt.l1_batch_tx_index.unwrap_or_default().into(),
        ))
    }

    async fn get_l2_to_l1_log_proof(
        &self,
        tx_hash: H256,
        l2_to_l1_log_index: Option<u64>,
    ) -> Result<Option<Proof>, ProviderError> {
        self.request(
            "zks_getL2ToL1LogProof",
            json!([tx_hash, l2_to_l1_log_index]),
        )
        .await
    }

    async fn get_transaction_receipt<T: Send + Sync + Into<TxHash>>(
        &self,
        transaction_hash: T,
    ) -> Result<Option<ZKTransactionReceipt>, ProviderError> {
        let hash = transaction_hash.into();
        let receipt: Result<Option<ZKTransactionReceipt>, ProviderError> =
            self.request("eth_getTransactionReceipt", [hash]).await;
        receipt
    }

    async fn get_withdrawal_l2_to_l1_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(u64, L2ToL1Log), ProviderError> {
        let receipt = ZKSProvider::get_transaction_receipt(self, withdrawal_hash).await?;

        if receipt.is_none() {
            return Err(ProviderError::CustomError(
                "Transaction is not mined!".into(),
            ));
        }

        let receipt = receipt.unwrap();
        let messages: Vec<(u64, L2ToL1Log)> = receipt
            .l2_to_l1_logs
            .into_iter()
            .enumerate()
            .filter(|(_, log)| log.sender == L1_MESSENGER_ADDRESS)
            .map(|(i, log)| (i as u64, log))
            .collect();

        messages.get(index).cloned().ok_or_else(|| {
            ProviderError::CustomError("L2ToL1Log not found at specified index".into())
        })
    }

    async fn get_finalize_withdrawal_params(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<FinalizeWithdrawalParams, ProviderError> {
        let (log, l1_batch_tx_id) = self.get_withdrawal_log(withdrawal_hash, index).await?;
        let (l2_to_l1_log_index, _) = self
            .get_withdrawal_l2_to_l1_log(withdrawal_hash, index)
            .await?;
        let sender = H160::from_slice(&log.topics[1][12..]);
        let proof = self
            .get_l2_to_l1_log_proof(withdrawal_hash, Some(l2_to_l1_log_index))
            .await?
            .ok_or_else(|| ProviderError::CustomError("Log proof not found!".into()))?;

        let message = ethers::abi::decode(&[ethers::abi::ParamType::Bytes], &log.data)
            .map_err(|e| ProviderError::CustomError(format!("Failed to decode log data: {}", e)))?
            .remove(0)
            .into_bytes()
            .ok_or_else(|| {
                ProviderError::CustomError("Failed to extract message from decoded data".into())
            })?;

        Ok(FinalizeWithdrawalParams {
            l2_batch_number: log.l1_batch_number.unwrap_or_default().into(),
            l2_message_index: proof.id.into(),
            l2_tx_number_in_block: l1_batch_tx_id,
            message: message.into(),
            sender,
            proof: proof,
        })
    }
}

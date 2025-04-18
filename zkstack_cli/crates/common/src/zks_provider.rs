use async_trait::async_trait;
use ethers::{
    providers::ProviderError,
    types::{Address, Bytes, H160, H256, U64},
};
use serde::{Deserialize, Serialize};
use zksync_system_constants::L1_MESSENGER_ADDRESS;
use zksync_types::{
    api::{L2ToL1Log, L2ToL1LogProof, Log},
    ethabi,
};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeWithdrawalParams {
    pub l2_batch_number: U64,
    pub l2_message_index: U64,
    pub l2_tx_number_in_block: U64,
    pub message: Bytes,
    pub sender: Address,
    pub proof: L2ToL1LogProof,
}

#[async_trait]
pub trait ZKSProvider {
    async fn get_withdrawal_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(Log, U64), ProviderError>;

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
}

#[async_trait]
impl ZKSProvider for Client<L2>
where
    Client<L2>: ZksNamespaceClient + EthNamespaceClient,
{
    async fn get_withdrawal_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(Log, U64), ProviderError> {
        let receipt = <Self as EthNamespaceClient>::get_transaction_receipt(self, withdrawal_hash)
            .await
            .map_err(|e| {
                ProviderError::CustomError(format!("Failed to get transaction receipt: {}", e))
            })?;

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
            Log {
                l1_batch_number: receipt.l1_batch_number,
                ..log
            },
            receipt.l1_batch_tx_index.unwrap_or_default(),
        ))
    }

    async fn get_withdrawal_l2_to_l1_log(
        &self,
        withdrawal_hash: H256,
        index: usize,
    ) -> Result<(u64, L2ToL1Log), ProviderError> {
        let receipt = <Self as EthNamespaceClient>::get_transaction_receipt(self, withdrawal_hash)
            .await
            .map_err(|e| {
                ProviderError::CustomError(format!("Failed to get withdrawal log: {}", e))
            })?;

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
        let proof = <Self as ZksNamespaceClient>::get_l2_to_l1_log_proof(
            self,
            withdrawal_hash,
            Some(l2_to_l1_log_index as usize),
        )
        .await
        .map_err(|e| {
            ProviderError::CustomError(format!("Failed to get withdrawal log proof: {}", e))
        })?
        .ok_or_else(|| ProviderError::CustomError("Log proof not found!".into()))?;

        let (sender, message) = get_message_from_log(&log)?;

        Ok(FinalizeWithdrawalParams {
            l2_batch_number: log.l1_batch_number.unwrap_or_default(),
            l2_message_index: proof.id.into(),
            l2_tx_number_in_block: l1_batch_tx_id,
            message,
            sender,
            proof,
        })
    }
}

pub fn get_message_from_log(log: &Log) -> Result<(H160, Bytes), ProviderError> {
    let sender = H160::from_slice(&log.topics[1][12..]);
    let message = ethers::abi::decode(&[ethers::abi::ParamType::Bytes], &log.data.0)
        .map_err(|e| ProviderError::CustomError(format!("Failed to decode log data: {}", e)))?
        .remove(0)
        .into_bytes()
        .ok_or_else(|| {
            ProviderError::CustomError("Failed to extract message from decoded data".into())
        })?;
    Ok((sender, message.into()))
}

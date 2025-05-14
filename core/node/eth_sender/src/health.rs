use serde::{Deserialize, Serialize};
use zksync_eth_client::ExecutedTxStatus;
use zksync_health_check::{Health, HealthStatus};
use zksync_types::{
    aggregated_operations::AggregatedActionType, eth_sender::EthTx, web3::TransactionReceipt,
    L1BlockNumber, Nonce, H256,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxStatus {
    pub tx_hash: H256,
    pub success: bool,
    pub receipt: TransactionReceipt,
}

impl From<&ExecutedTxStatus> for TxStatus {
    fn from(status: &ExecutedTxStatus) -> Self {
        Self {
            tx_hash: status.tx_hash,
            success: status.success,
            receipt: status.receipt.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EthTxAggregatorHealthDetails {
    pub last_saved_tx: EthTxDetails,
}

impl From<EthTxAggregatorHealthDetails> for Health {
    fn from(details: EthTxAggregatorHealthDetails) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EthTxDetails {
    pub nonce: Nonce,
    pub tx_type: AggregatedActionType,
    pub created_at_timestamp: u64,
    pub predicted_gas_cost: Option<u64>,
    pub status: Option<TxStatus>,
}

impl EthTxDetails {
    pub fn new(tx: &EthTx, status: Option<TxStatus>) -> Self {
        Self {
            nonce: tx.nonce,
            tx_type: tx.tx_type,
            created_at_timestamp: tx.created_at_timestamp,
            predicted_gas_cost: tx.predicted_gas_cost,
            status,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EthTxManagerHealthDetails {
    pub last_mined_tx: EthTxDetails,
    pub finalized_block: L1BlockNumber,
}

impl From<EthTxManagerHealthDetails> for Health {
    fn from(details: EthTxManagerHealthDetails) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

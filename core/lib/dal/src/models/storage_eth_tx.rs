use sqlx::types::chrono::NaiveDateTime;
use std::str::FromStr;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::eth_sender::{EthTx, TxHistory, TxHistoryToSend};
use zksync_types::{Address, L1BatchNumber, Nonce, H256};

#[derive(Debug, Clone)]
pub struct StorageEthTx {
    pub id: i32,
    pub nonce: i64,
    pub contract_address: String,
    pub raw_tx: Vec<u8>,
    pub tx_type: String,
    pub has_failed: bool,
    pub confirmed_eth_tx_history_id: Option<i32>,
    pub gas_used: Option<i64>,
    pub predicted_gas_cost: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    // TODO (SMA-1614): remove the field
    pub sent_at_block: Option<i32>,
}

#[derive(Debug, Default)]
pub struct L1BatchEthSenderStats {
    pub saved: Vec<(AggregatedActionType, L1BatchNumber)>,
    pub mined: Vec<(AggregatedActionType, L1BatchNumber)>,
}

#[derive(Clone, Debug)]
pub struct StorageTxHistoryToSend {
    pub id: i32,
    pub eth_tx_id: i32,
    pub tx_hash: String,
    pub priority_fee_per_gas: i64,
    pub base_fee_per_gas: i64,
    pub signed_raw_tx: Option<Vec<u8>>,
    pub nonce: i64,
}

#[derive(Clone, Debug)]
pub struct StorageTxHistory {
    pub id: i32,
    pub eth_tx_id: i32,
    pub priority_fee_per_gas: i64,
    pub base_fee_per_gas: i64,
    pub tx_hash: String,
    pub confirmed_at: Option<NaiveDateTime>,
    pub sent_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub signed_raw_tx: Option<Vec<u8>>,
    pub sent_at_block: Option<i32>,
}

impl From<StorageEthTx> for EthTx {
    fn from(tx: StorageEthTx) -> EthTx {
        EthTx {
            id: tx.id as u32,
            nonce: Nonce(tx.nonce as u32),
            contract_address: Address::from_str(&tx.contract_address)
                .expect("Incorrect address in db"),
            raw_tx: tx.raw_tx.clone(),
            tx_type: AggregatedActionType::from_str(&tx.tx_type).expect("Wrong agg type"),
            created_at_timestamp: tx.created_at.timestamp() as u64,
            predicted_gas_cost: tx.predicted_gas_cost as u64,
        }
    }
}

impl From<StorageTxHistory> for TxHistory {
    fn from(history: StorageTxHistory) -> TxHistory {
        TxHistory {
            id: history.id as u32,
            eth_tx_id: history.eth_tx_id as u32,
            base_fee_per_gas: history.base_fee_per_gas as u64,
            priority_fee_per_gas: history.priority_fee_per_gas as u64,
            tx_hash: H256::from_str(&history.tx_hash).expect("Incorrect hash"),
            signed_raw_tx: history
                .signed_raw_tx
                .expect("Should rely only on the new txs"),

            sent_at_block: history.sent_at_block.map(|block| block as u32),
        }
    }
}

impl From<StorageTxHistoryToSend> for TxHistoryToSend {
    fn from(history: StorageTxHistoryToSend) -> TxHistoryToSend {
        TxHistoryToSend {
            id: history.id as u32,
            eth_tx_id: history.eth_tx_id as u32,
            tx_hash: H256::from_str(&history.tx_hash).expect("Incorrect hash"),
            base_fee_per_gas: history.base_fee_per_gas as u64,
            priority_fee_per_gas: history.priority_fee_per_gas as u64,
            signed_raw_tx: history
                .signed_raw_tx
                .expect("Should rely only on the new txs"),
            nonce: Nonce(history.nonce as u32),
        }
    }
}

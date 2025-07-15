use std::str::FromStr;

use sqlx::types::chrono::NaiveDateTime;
use zksync_types::{
    aggregated_operations::{AggregatedActionType, AggregatedOperationType},
    eth_sender::{EthTx, TxHistory},
    Address, L1BatchNumber, Nonce, SLChainId, H256,
};

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
    pub predicted_gas_cost: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    // TODO (SMA-1614): remove the field
    pub sent_at_block: Option<i32>,
    // If this field is `Some` this means that this transaction was sent by a custom operator
    // such as blob sender operator.
    pub from_addr: Option<Vec<u8>>,
    // A `EIP_4844_TX_TYPE` transaction blob sidecar.
    //
    // Format a `bincode`-encoded `EthTxBlobSidecar` enum.
    pub blob_sidecar: Option<Vec<u8>>,
    pub is_gateway: bool,
    pub chain_id: Option<i64>,
}

#[derive(Debug, Default)]
pub struct L1BatchEthSenderStats {
    pub saved: Vec<(AggregatedOperationType, L1BatchNumber)>,
    pub mined: Vec<(AggregatedOperationType, L1BatchNumber)>,
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
    // A `EIP_4844_TX_TYPE` transaction blob sidecar.
    //
    // Format a `bincode`-encoded `EthTxBlobSidecar` enum.
    pub blob_sidecar: Option<Vec<u8>>,
    pub blob_base_fee_per_gas: Option<i64>,

    // EIP712 txs
    pub max_gas_per_pubdata: Option<i64>,
    pub predicted_gas_limit: Option<i64>,
    pub sent_successfully: bool,
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
            created_at_timestamp: tx.created_at.and_utc().timestamp() as u64,
            predicted_gas_cost: tx.predicted_gas_cost.map(|c| c as u64),
            from_addr: tx.from_addr.map(|f| Address::from_slice(&f)),
            blob_sidecar: tx.blob_sidecar.map(|b| {
                bincode::deserialize(&b).expect("EthTxBlobSidecar is encoded correctly; qed")
            }),
            is_gateway: tx.is_gateway,
            chain_id: tx
                .chain_id
                .map(|chain_id| SLChainId(chain_id.try_into().unwrap())),
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
            blob_base_fee_per_gas: history.blob_base_fee_per_gas.map(|v| v as u64),
            tx_hash: H256::from_str(&history.tx_hash).expect("Incorrect hash"),
            signed_raw_tx: history
                .signed_raw_tx
                .expect("Should rely only on the new txs"),

            sent_at_block: history.sent_at_block.map(|block| block as u32),
            max_gas_per_pubdata: history.max_gas_per_pubdata.map(|v| v as u64),
            sent_successfully: history.sent_successfully,
        }
    }
}

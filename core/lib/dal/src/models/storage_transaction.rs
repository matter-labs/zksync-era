use std::{convert::TryInto, str::FromStr};

use crate::BigDecimal;
use bigdecimal::Zero;

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::types::chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Error, FromRow, Row};

use zksync_types::l2::TransactionType;
use zksync_types::protocol_version::ProtocolUpgradeTxCommonData;
use zksync_types::transaction_request::PaymasterParams;
use zksync_types::vm_trace::Call;
use zksync_types::web3::types::U64;
use zksync_types::{api, Bytes, ExecuteTransactionCommon};
use zksync_types::{
    api::{TransactionDetails, TransactionStatus},
    fee::Fee,
    l1::{OpProcessingType, PriorityQueueType},
    Address, Execute, L1TxCommonData, L2ChainId, L2TxCommonData, Nonce, PackedEthSignature,
    PriorityOpId, Transaction, EIP_1559_TX_TYPE, EIP_2930_TX_TYPE, EIP_712_TX_TYPE, H160, H256,
    PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use zksync_utils::bigdecimal_to_u256;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTransaction {
    pub priority_op_id: Option<i64>,
    pub hash: Vec<u8>,
    pub is_priority: bool,
    pub full_fee: Option<BigDecimal>,
    pub layer_2_tip_fee: Option<BigDecimal>,
    pub initiator_address: Vec<u8>,
    pub nonce: Option<i64>,
    pub signature: Option<Vec<u8>>,
    pub gas_limit: Option<BigDecimal>,
    pub max_fee_per_gas: Option<BigDecimal>,
    pub max_priority_fee_per_gas: Option<BigDecimal>,
    pub gas_per_storage_limit: Option<BigDecimal>,
    pub gas_per_pubdata_limit: Option<BigDecimal>,
    pub input: Option<Vec<u8>>,
    pub tx_format: Option<i32>,
    pub data: serde_json::Value,
    pub received_at: NaiveDateTime,
    pub in_mempool: bool,

    pub l1_block_number: Option<i32>,
    pub l1_batch_number: Option<i64>,
    pub l1_batch_tx_index: Option<i32>,
    pub miniblock_number: Option<i64>,
    pub index_in_block: Option<i32>,
    pub error: Option<String>,
    pub effective_gas_price: Option<BigDecimal>,
    pub contract_address: Option<Vec<u8>>,
    pub value: BigDecimal,

    pub paymaster: Vec<u8>,
    pub paymaster_input: Vec<u8>,

    pub refunded_gas: i64,

    pub execution_info: serde_json::Value,

    pub l1_tx_mint: Option<BigDecimal>,
    pub l1_tx_refund_recipient: Option<Vec<u8>>,

    pub upgrade_id: Option<i32>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl From<StorageTransaction> for L1TxCommonData {
    fn from(tx: StorageTransaction) -> Self {
        let gas_limit = {
            let gas_limit_string = tx
                .gas_limit
                .as_ref()
                .expect("gas limit is mandatory for transaction")
                .to_string();

            U256::from_dec_str(&gas_limit_string)
                .unwrap_or_else(|_| panic!("Incorrect gas limit value in DB {}", gas_limit_string))
        };

        let full_fee = {
            let full_fee_string = tx
                .full_fee
                .expect("full fee is mandatory for priority operation")
                .to_string();

            U256::from_dec_str(&full_fee_string)
                .unwrap_or_else(|_| panic!("Incorrect full fee value in DB {}", full_fee_string))
        };

        let layer_2_tip_fee = {
            let layer_2_tip_fee_string = tx
                .layer_2_tip_fee
                .expect("layer 2 tip fee is mandatory for priority operation")
                .to_string();

            U256::from_dec_str(&layer_2_tip_fee_string).unwrap_or_else(|_| {
                panic!(
                    "Incorrect layer 2 tip fee value in DB {}",
                    layer_2_tip_fee_string
                )
            })
        };

        // Supporting None for compatibility with the old transactions
        let to_mint = tx.l1_tx_mint.map(bigdecimal_to_u256).unwrap_or_default();
        // Supporting None for compatibility with the old transactions
        let refund_recipient = tx
            .l1_tx_refund_recipient
            .map(|recipient| Address::from_slice(&recipient))
            .unwrap_or_default();

        // `tx.hash` represents the transaction hash obtained from the execution results,
        // and it should be exactly the same as the canonical tx hash calculated from the
        // transaction data, so we don't store it as a separate "canonical_tx_hash" field.
        let canonical_tx_hash = H256::from_slice(&tx.hash);

        L1TxCommonData {
            full_fee,
            layer_2_tip_fee,
            priority_queue_type: PriorityQueueType::Deque,
            op_processing_type: OpProcessingType::Common,
            sender: Address::from_slice(&tx.initiator_address),
            serial_id: PriorityOpId(tx.priority_op_id.unwrap() as u64),
            gas_limit,
            max_fee_per_gas: tx
                .max_fee_per_gas
                .map(bigdecimal_to_u256)
                .unwrap_or_default(),
            to_mint,
            refund_recipient,
            // Using 1 for old transactions that did not have the necessary field stored
            gas_per_pubdata_limit: tx
                .gas_per_pubdata_limit
                .map(bigdecimal_to_u256)
                .unwrap_or_else(|| U256::from(1u32)),
            deadline_block: 0,
            eth_hash: Default::default(),
            eth_block: tx.l1_block_number.unwrap_or_default() as u64,
            canonical_tx_hash,
        }
    }
}

impl From<StorageTransaction> for L2TxCommonData {
    fn from(tx: StorageTransaction) -> Self {
        let gas_limit = {
            let gas_limit_string = tx
                .gas_limit
                .as_ref()
                .expect("gas limit is mandatory for transaction")
                .to_string();

            U256::from_dec_str(&gas_limit_string)
                .unwrap_or_else(|_| panic!("Incorrect gas limit value in DB {}", gas_limit_string))
        };
        let nonce = Nonce(tx.nonce.expect("no nonce in L2 tx in DB") as u32);
        let max_fee_per_gas = {
            let max_fee_per_gas_string = tx
                .max_fee_per_gas
                .as_ref()
                .expect("max price per gas is mandatory for transaction")
                .to_string();

            U256::from_dec_str(&max_fee_per_gas_string).unwrap_or_else(|_| {
                panic!(
                    "Incorrect max price per gas value in DB {}",
                    max_fee_per_gas_string
                )
            })
        };

        let max_priority_fee_per_gas = {
            let max_priority_fee_per_gas_string = tx
                .max_priority_fee_per_gas
                .as_ref()
                .expect("max priority fee per gas is mandatory for transaction")
                .to_string();

            U256::from_dec_str(&max_priority_fee_per_gas_string).unwrap_or_else(|_| {
                panic!(
                    "Incorrect max priority fee per gas value in DB {}",
                    max_priority_fee_per_gas_string
                )
            })
        };

        let gas_per_pubdata_limit = {
            let gas_per_pubdata_limit_string = tx
                .gas_per_pubdata_limit
                .as_ref()
                .expect("gas price per pubdata limit is mandatory for transaction")
                .to_string();
            U256::from_dec_str(&gas_per_pubdata_limit_string).unwrap_or_else(|_| {
                panic!(
                    "Incorrect gas price per pubdata limit value in DB {}",
                    gas_per_pubdata_limit_string
                )
            })
        };

        let fee = Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_per_pubdata_limit,
        };

        let tx_format = match tx.tx_format.map(|a| a as u8) {
            Some(EIP_712_TX_TYPE) => TransactionType::EIP712Transaction,
            Some(EIP_2930_TX_TYPE) => TransactionType::EIP2930Transaction,
            Some(EIP_1559_TX_TYPE) => TransactionType::EIP1559Transaction,
            Some(0) | None => TransactionType::LegacyTransaction,
            Some(_) => unreachable!("Unsupported tx type"),
        };

        let StorageTransaction {
            paymaster,
            paymaster_input,
            initiator_address,
            signature,
            hash,
            input,
            ..
        } = tx;

        let paymaster_params = PaymasterParams {
            paymaster: Address::from_slice(&paymaster),
            paymaster_input,
        };

        L2TxCommonData::new(
            nonce,
            fee,
            Address::from_slice(&initiator_address),
            signature.unwrap_or_else(|| {
                panic!("Signature is mandatory for transactions. Tx {:#?}", hash)
            }),
            tx_format,
            input.expect("input data is mandatory for l2 transactions"),
            H256::from_slice(&hash),
            paymaster_params,
        )
    }
}

impl From<StorageTransaction> for ProtocolUpgradeTxCommonData {
    fn from(tx: StorageTransaction) -> Self {
        let gas_limit = {
            let gas_limit_string = tx
                .gas_limit
                .as_ref()
                .expect("gas limit is mandatory for transaction")
                .to_string();

            U256::from_dec_str(&gas_limit_string)
                .unwrap_or_else(|_| panic!("Incorrect gas limit value in DB {}", gas_limit_string))
        };

        let to_mint = tx.l1_tx_mint.map(bigdecimal_to_u256).unwrap_or_default();
        let refund_recipient = tx
            .l1_tx_refund_recipient
            .map(|recipient| Address::from_slice(&recipient))
            .unwrap_or_default();
        let canonical_tx_hash = H256::from_slice(&tx.hash);

        ProtocolUpgradeTxCommonData {
            sender: Address::from_slice(&tx.initiator_address),
            upgrade_id: (tx.upgrade_id.unwrap() as u16).try_into().unwrap(),
            gas_limit,
            max_fee_per_gas: tx
                .max_fee_per_gas
                .map(bigdecimal_to_u256)
                .unwrap_or_default(),
            to_mint,
            refund_recipient,
            // Using 1 for old transactions that did not have the necessary field stored
            gas_per_pubdata_limit: tx
                .gas_per_pubdata_limit
                .map(bigdecimal_to_u256)
                .expect("gas_per_pubdata_limit field is missing for protocol upgrade tx"),
            eth_hash: Default::default(),
            eth_block: tx.l1_block_number.unwrap_or_default() as u64,
            canonical_tx_hash,
        }
    }
}

impl From<StorageTransaction> for Transaction {
    fn from(tx: StorageTransaction) -> Self {
        let hash = H256::from_slice(&tx.hash);
        let execute = serde_json::from_value::<Execute>(tx.data.clone())
            .unwrap_or_else(|_| panic!("invalid json in database for tx {:?}", hash));
        let received_timestamp_ms = tx.received_at.timestamp_millis() as u64;
        match tx.tx_format {
            Some(t) if t == PRIORITY_OPERATION_L2_TX_TYPE as i32 => Transaction {
                common_data: ExecuteTransactionCommon::L1(tx.into()),
                execute,
                received_timestamp_ms,
                raw_bytes: None,
            },
            Some(t) if t == PROTOCOL_UPGRADE_TX_TYPE as i32 => Transaction {
                common_data: ExecuteTransactionCommon::ProtocolUpgrade(tx.into()),
                execute,
                received_timestamp_ms,
                raw_bytes: None,
            },
            _ => Transaction {
                raw_bytes: tx.input.clone().map(Bytes::from),
                common_data: ExecuteTransactionCommon::L2(tx.into()),
                execute,
                received_timestamp_ms,
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct StorageApiTransaction {
    #[serde(flatten)]
    pub inner_api_transaction: api::Transaction,
}

impl From<StorageApiTransaction> for api::Transaction {
    fn from(tx: StorageApiTransaction) -> Self {
        tx.inner_api_transaction
    }
}

impl<'r> FromRow<'r, PgRow> for StorageApiTransaction {
    fn from_row(db_row: &'r PgRow) -> Result<Self, Error> {
        let row_signature: Option<Vec<u8>> = db_row.get("signature");
        let signature = row_signature
            .and_then(|signature| PackedEthSignature::deserialize_packed(&signature).ok());

        Ok(StorageApiTransaction {
            inner_api_transaction: api::Transaction {
                hash: H256::from_slice(db_row.get("tx_hash")),
                nonce: U256::from(db_row.try_get::<i64, &str>("nonce").ok().unwrap_or(0)),
                block_hash: db_row.try_get("block_hash").ok().map(H256::from_slice),
                block_number: db_row
                    .try_get::<i64, &str>("block_number")
                    .ok()
                    .map(U64::from),
                transaction_index: db_row
                    .try_get::<i32, &str>("index_in_block")
                    .ok()
                    .map(U64::from),
                from: Some(H160::from_slice(db_row.get("initiator_address"))),
                to: Some(
                    serde_json::from_value::<Address>(db_row.get("execute_contract_address"))
                        .expect("incorrect address value in the database"),
                ),
                value: bigdecimal_to_u256(db_row.get::<BigDecimal, &str>("value")),
                // `gas_price`, `max_fee_per_gas`, `max_priority_fee_per_gas` will be zero for the priority transactions.
                // For common L2 transactions `gas_price` is equal to `effective_gas_price` if the transaction is included
                // in some block, or `max_fee_per_gas` otherwise.
                gas_price: Some(bigdecimal_to_u256(
                    db_row
                        .try_get::<BigDecimal, &str>("effective_gas_price")
                        .or_else(|_| db_row.try_get::<BigDecimal, &str>("max_fee_per_gas"))
                        .unwrap_or_else(|_| BigDecimal::zero()),
                )),
                max_fee_per_gas: Some(bigdecimal_to_u256(
                    db_row
                        .try_get::<BigDecimal, &str>("max_fee_per_gas")
                        .unwrap_or_else(|_| BigDecimal::zero()),
                )),
                max_priority_fee_per_gas: Some(bigdecimal_to_u256(
                    db_row
                        .try_get::<BigDecimal, &str>("max_priority_fee_per_gas")
                        .unwrap_or_else(|_| BigDecimal::zero()),
                )),
                gas: bigdecimal_to_u256(db_row.get::<BigDecimal, &str>("gas_limit")),
                input: serde_json::from_value(db_row.get::<serde_json::Value, &str>("calldata"))
                    .expect("Incorrect calldata value in the database"),
                raw: None,
                v: signature.as_ref().map(|s| U64::from(s.v())),
                r: signature.as_ref().map(|s| U256::from(s.r())),
                s: signature.as_ref().map(|s| U256::from(s.s())),
                transaction_type: db_row
                    .try_get::<Option<i32>, &str>("tx_format")
                    .unwrap_or_default()
                    .map(U64::from),
                access_list: None,
                chain_id: U256::zero(),
                l1_batch_number: db_row
                    .try_get::<i64, &str>("l1_batch_number_tx")
                    .ok()
                    .map(U64::from),
                l1_batch_tx_index: db_row
                    .try_get::<i32, &str>("l1_batch_tx_index")
                    .ok()
                    .map(U64::from),
            },
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTransactionDetails {
    pub is_priority: bool,
    pub initiator_address: Vec<u8>,
    pub gas_limit: Option<BigDecimal>,
    pub gas_per_pubdata_limit: Option<BigDecimal>,
    pub received_at: NaiveDateTime,
    pub miniblock_number: Option<i64>,
    pub error: Option<String>,
    pub effective_gas_price: Option<BigDecimal>,
    pub refunded_gas: i64,
    pub eth_commit_tx_hash: Option<String>,
    pub eth_prove_tx_hash: Option<String>,
    pub eth_execute_tx_hash: Option<String>,
}

impl StorageTransactionDetails {
    fn get_transaction_status(&self) -> TransactionStatus {
        if self.error.is_some() {
            TransactionStatus::Failed
        } else if self.eth_execute_tx_hash.is_some() {
            TransactionStatus::Verified
        } else if self.miniblock_number.is_some() {
            TransactionStatus::Included
        } else {
            TransactionStatus::Pending
        }
    }
}

impl From<StorageTransactionDetails> for TransactionDetails {
    fn from(tx_details: StorageTransactionDetails) -> Self {
        let status = tx_details.get_transaction_status();

        let effective_gas_price =
            bigdecimal_to_u256(tx_details.effective_gas_price.unwrap_or_default());

        let gas_limit = bigdecimal_to_u256(
            tx_details
                .gas_limit
                .expect("gas limit is mandatory for transaction"),
        );
        let gas_refunded = U256::from(tx_details.refunded_gas as u32);
        let fee = (gas_limit - gas_refunded) * effective_gas_price;

        let gas_per_pubdata =
            bigdecimal_to_u256(tx_details.gas_per_pubdata_limit.unwrap_or_default());

        let initiator_address = H160::from_slice(tx_details.initiator_address.as_slice());
        let received_at = DateTime::<Utc>::from_naive_utc_and_offset(tx_details.received_at, Utc);

        let eth_commit_tx_hash = tx_details
            .eth_commit_tx_hash
            .map(|hash| H256::from_str(&hash).unwrap());
        let eth_prove_tx_hash = tx_details
            .eth_prove_tx_hash
            .map(|hash| H256::from_str(&hash).unwrap());
        let eth_execute_tx_hash = tx_details
            .eth_execute_tx_hash
            .map(|hash| H256::from_str(&hash).unwrap());

        TransactionDetails {
            is_l1_originated: tx_details.is_priority,
            status,
            fee,
            gas_per_pubdata,
            initiator_address,
            received_at,
            eth_commit_tx_hash,
            eth_prove_tx_hash,
            eth_execute_tx_hash,
        }
    }
}

pub fn web3_transaction_select_sql() -> &'static str {
    r#"
         transactions.hash as tx_hash,
         transactions.index_in_block as index_in_block,
         transactions.miniblock_number as block_number,
         transactions.nonce as nonce,
         transactions.signature as signature,
         transactions.initiator_address as initiator_address,
         transactions.tx_format as tx_format,
         transactions.value as value,
         transactions.gas_limit as gas_limit,
         transactions.max_fee_per_gas as max_fee_per_gas,
         transactions.max_priority_fee_per_gas as max_priority_fee_per_gas,
         transactions.effective_gas_price as effective_gas_price,
         transactions.l1_batch_number as l1_batch_number_tx,
         transactions.l1_batch_tx_index as l1_batch_tx_index,
         transactions.data->'contractAddress' as "execute_contract_address",
         transactions.data->'calldata' as "calldata",
         miniblocks.hash as "block_hash"
    "#
}

pub fn extract_web3_transaction(db_row: PgRow, chain_id: L2ChainId) -> api::Transaction {
    let mut storage_api_tx = StorageApiTransaction::from_row(&db_row).unwrap();
    storage_api_tx.inner_api_transaction.chain_id = U256::from(chain_id.as_u64());
    if storage_api_tx.inner_api_transaction.transaction_type == Some(U64::from(0)) {
        storage_api_tx.inner_api_transaction.v = storage_api_tx
            .inner_api_transaction
            .v
            .map(|v| v + 35 + chain_id.as_u64() * 2);
    }
    storage_api_tx.into()
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CallTrace {
    pub tx_hash: Vec<u8>,
    pub call_trace: Vec<u8>,
}

impl From<CallTrace> for Call {
    fn from(call_trace: CallTrace) -> Self {
        bincode::deserialize(&call_trace.call_trace).unwrap()
    }
}

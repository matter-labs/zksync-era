use std::{convert::TryInto, str::FromStr};

use bigdecimal::Zero;
use sqlx::types::chrono::{DateTime, NaiveDateTime, Utc};
use zksync_types::{
    api,
    api::{TransactionDetails, TransactionReceipt, TransactionStatus},
    fee::Fee,
    l1::{OpProcessingType, PriorityQueueType},
    l2::TransactionType,
    protocol_version::ProtocolUpgradeTxCommonData,
    transaction_request::PaymasterParams,
    vm_trace::Call,
    web3::types::U64,
    Address, Bytes, Execute, ExecuteTransactionCommon, L1TxCommonData, L2ChainId, L2TxCommonData,
    Nonce, PackedEthSignature, PriorityOpId, Transaction, EIP_1559_TX_TYPE, EIP_2930_TX_TYPE,
    EIP_712_TX_TYPE, H160, H256, PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use zksync_utils::{bigdecimal_to_u256, h256_to_account_address};

use crate::BigDecimal;

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
        // transaction data, so we don't store it as a separate `canonical_tx_hash` field.
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

#[derive(sqlx::FromRow)]
pub(crate) struct StorageTransactionReceipt {
    pub error: Option<String>,
    pub tx_format: Option<i32>,
    pub index_in_block: Option<i32>,
    pub block_hash: Vec<u8>,
    pub tx_hash: Vec<u8>,
    pub block_number: i64,
    pub l1_batch_tx_index: Option<i32>,
    pub l1_batch_number: Option<i64>,
    pub transfer_to: Option<serde_json::Value>,
    pub execute_contract_address: Option<serde_json::Value>,
    pub refunded_gas: i64,
    pub gas_limit: Option<BigDecimal>,
    pub effective_gas_price: Option<BigDecimal>,
    pub contract_address: Option<Vec<u8>>,
    pub initiator_address: Vec<u8>,
}

impl From<StorageTransactionReceipt> for TransactionReceipt {
    fn from(storage_receipt: StorageTransactionReceipt) -> Self {
        let status = storage_receipt.error.map_or_else(U64::one, |_| U64::zero());

        let tx_type = storage_receipt
            .tx_format
            .map_or_else(Default::default, U64::from);
        let transaction_index = storage_receipt
            .index_in_block
            .map_or_else(Default::default, U64::from);

        let block_hash = H256::from_slice(&storage_receipt.block_hash);
        TransactionReceipt {
            transaction_hash: H256::from_slice(&storage_receipt.tx_hash),
            transaction_index,
            block_hash,
            block_number: storage_receipt.block_number.into(),
            l1_batch_tx_index: storage_receipt.l1_batch_tx_index.map(U64::from),
            l1_batch_number: storage_receipt.l1_batch_number.map(U64::from),
            from: H160::from_slice(&storage_receipt.initiator_address),
            to: storage_receipt
                .transfer_to
                .or(storage_receipt.execute_contract_address)
                .map(|addr| {
                    serde_json::from_value::<Address>(addr)
                        .expect("invalid address value in the database")
                })
                // For better compatibility with various clients, we never return null.
                .or_else(|| Some(Address::default())),
            cumulative_gas_used: Default::default(), // TODO: Should be actually calculated (SMA-1183).
            gas_used: {
                let refunded_gas: U256 = storage_receipt.refunded_gas.into();
                storage_receipt.gas_limit.map(|val| {
                    let gas_limit = bigdecimal_to_u256(val);
                    gas_limit - refunded_gas
                })
            },
            effective_gas_price: Some(
                storage_receipt
                    .effective_gas_price
                    .map(bigdecimal_to_u256)
                    .unwrap_or_default(),
            ),
            contract_address: storage_receipt
                .contract_address
                .map(|addr| h256_to_account_address(&H256::from_slice(&addr))),
            logs: vec![],
            l2_to_l1_logs: vec![],
            status,
            root: block_hash,
            logs_bloom: Default::default(),
            // Even though the Rust SDK recommends us to supply "None" for legacy transactions
            // we always supply some number anyway to have the same behavior as most popular RPCs
            transaction_type: Some(tx_type),
        }
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

#[derive(Debug)]
pub(crate) struct StorageApiTransaction {
    pub tx_hash: Vec<u8>,
    pub index_in_block: Option<i32>,
    pub block_number: Option<i64>,
    pub nonce: Option<i64>,
    pub signature: Option<Vec<u8>>,
    pub initiator_address: Vec<u8>,
    pub tx_format: Option<i32>,
    pub value: BigDecimal,
    pub gas_limit: Option<BigDecimal>,
    pub max_fee_per_gas: Option<BigDecimal>,
    pub max_priority_fee_per_gas: Option<BigDecimal>,
    pub effective_gas_price: Option<BigDecimal>,
    pub l1_batch_number: Option<i64>,
    pub l1_batch_tx_index: Option<i32>,
    pub execute_contract_address: serde_json::Value,
    pub calldata: serde_json::Value,
    pub block_hash: Option<Vec<u8>>,
}

impl StorageApiTransaction {
    pub fn into_api(self, chain_id: L2ChainId) -> api::Transaction {
        let signature = self
            .signature
            .and_then(|signature| PackedEthSignature::deserialize_packed(&signature).ok());

        let mut tx = api::Transaction {
            hash: H256::from_slice(&self.tx_hash),
            nonce: U256::from(self.nonce.unwrap_or(0) as u64),
            block_hash: self.block_hash.map(|hash| H256::from_slice(&hash)),
            block_number: self.block_number.map(|number| U64::from(number as u64)),
            transaction_index: self.index_in_block.map(|idx| U64::from(idx as u64)),
            from: Some(Address::from_slice(&self.initiator_address)),
            to: Some(serde_json::from_value(self.execute_contract_address).unwrap()),
            value: bigdecimal_to_u256(self.value),
            gas_price: Some(bigdecimal_to_u256(
                self.effective_gas_price
                    .or_else(|| self.max_fee_per_gas.clone())
                    .unwrap_or_else(BigDecimal::zero),
            )),
            gas: bigdecimal_to_u256(self.gas_limit.unwrap_or_else(BigDecimal::zero)),
            input: serde_json::from_value(self.calldata).expect("incorrect calldata in Postgres"),
            v: signature.as_ref().map(|s| U64::from(s.v())),
            r: signature.as_ref().map(|s| U256::from(s.r())),
            s: signature.as_ref().map(|s| U256::from(s.s())),
            raw: None,
            transaction_type: self.tx_format.map(|format| U64::from(format as u32)),
            access_list: None,
            max_fee_per_gas: Some(bigdecimal_to_u256(
                self.max_fee_per_gas.unwrap_or_else(BigDecimal::zero),
            )),
            max_priority_fee_per_gas: Some(bigdecimal_to_u256(
                self.max_priority_fee_per_gas
                    .unwrap_or_else(BigDecimal::zero),
            )),
            chain_id: U256::from(chain_id.as_u64()),
            l1_batch_number: self.l1_batch_number.map(|number| U64::from(number as u64)),
            l1_batch_tx_index: self.l1_batch_tx_index.map(|idx| U64::from(idx as u64)),
        };

        if tx.transaction_type == Some(U64::from(0)) {
            tx.v = tx.v.map(|v| v + 35 + chain_id.as_u64() * 2);
        }
        tx
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CallTrace {
    pub call_trace: Vec<u8>,
}

impl From<CallTrace> for Call {
    fn from(call_trace: CallTrace) -> Self {
        bincode::deserialize(&call_trace.call_trace).unwrap()
    }
}

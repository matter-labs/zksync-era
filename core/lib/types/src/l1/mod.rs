//! Definition of zkSync network priority operations: operations initiated from the L1.

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use zksync_basic_types::{
    ethabi::{decode, ParamType, Token},
    Address, L1BlockNumber, Log, PriorityOpId, H160, H256, U256,
};
use zksync_utils::u256_to_account_address;

use crate::{
    helpers::unix_timestamp_ms,
    l1::error::L1TxParseError,
    l2::TransactionType,
    priority_op_onchain_data::{PriorityOpOnchainData, PriorityOpOnchainMetadata},
    tx::Execute,
    ExecuteTransactionCommon, PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE,
};

use super::Transaction;

pub mod error;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy)]
#[repr(u8)]
pub enum OpProcessingType {
    Common = 0,
    OnlyRollup = 1,
}

impl TryFrom<u8> for OpProcessingType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == OpProcessingType::Common as u8 => Ok(OpProcessingType::Common),
            x if x == OpProcessingType::OnlyRollup as u8 => Ok(OpProcessingType::OnlyRollup),
            _ => Err(()),
        }
    }
}

impl Default for OpProcessingType {
    fn default() -> Self {
        Self::Common
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy, Default)]
#[repr(u8)]
pub enum PriorityQueueType {
    #[default]
    Deque = 0,
    HeapBuffer = 1,
    Heap = 2,
}

impl TryFrom<u8> for PriorityQueueType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == PriorityQueueType::Deque as u8 => Ok(PriorityQueueType::Deque),
            x if x == PriorityQueueType::HeapBuffer as u8 => Ok(PriorityQueueType::HeapBuffer),
            x if x == PriorityQueueType::Heap as u8 => Ok(PriorityQueueType::Heap),
            _ => Err(()),
        }
    }
}

pub fn is_l1_tx_type(tx_type: u8) -> bool {
    tx_type == PRIORITY_OPERATION_L2_TX_TYPE || tx_type == PROTOCOL_UPGRADE_TX_TYPE
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1TxCommonData {
    /// Sender of the transaction.
    pub sender: Address,
    /// Unique ID of the priority operation.
    pub serial_id: PriorityOpId,
    /// Ethereum deadline block until which operation must be processed.
    pub deadline_block: u64,
    /// Additional payment to the operator as an incentive to perform the operation. The contract uses a value of 192 bits.
    pub layer_2_tip_fee: U256,
    /// The total cost the sender paid for the transaction.
    pub full_fee: U256,
    /// The maximal fee per gas to be used for L1->L2 transaction
    pub max_fee_per_gas: U256,
    /// The maximum number of gas that a transaction can spend at a price of gas equals 1.
    pub gas_limit: U256,
    /// The maximum number of gas per 1 byte of pubdata.
    pub gas_per_pubdata_limit: U256,
    /// Indicator that the operation can interact with Rollup and Porter trees, or only with Rollup.
    pub op_processing_type: OpProcessingType,
    /// Priority operations queue type.
    pub priority_queue_type: PriorityQueueType,
    /// Hash of the corresponding Ethereum transaction. Size should be 32 bytes.
    pub eth_hash: H256,
    /// Block in which Ethereum transaction was included.
    pub eth_block: u64,
    /// Tx hash of the transaction in the zkSync network. Calculated as the encoded transaction data hash.
    pub canonical_tx_hash: H256,
    /// The amount of ETH that should be minted with this transaction
    pub to_mint: U256,
    /// The recipient of the refund of the transaction
    pub refund_recipient: Address,
}

impl L1TxCommonData {
    pub fn hash(&self) -> H256 {
        self.canonical_tx_hash
    }

    pub fn onchain_data(&self) -> PriorityOpOnchainData {
        PriorityOpOnchainData {
            layer_2_tip_fee: self.layer_2_tip_fee,
            onchain_data_hash: self.hash(),
        }
    }

    pub fn onchain_metadata(&self) -> PriorityOpOnchainMetadata {
        PriorityOpOnchainMetadata {
            op_processing_type: self.op_processing_type,
            priority_queue_type: self.priority_queue_type,
            onchain_data: self.onchain_data(),
        }
    }

    pub fn tx_format(&self) -> TransactionType {
        TransactionType::PriorityOpTransaction
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Tx {
    pub execute: Execute,
    pub common_data: L1TxCommonData,
    pub received_timestamp_ms: u64,
}

impl From<L1Tx> for Transaction {
    fn from(tx: L1Tx) -> Self {
        let L1Tx {
            execute,
            common_data,
            received_timestamp_ms,
        } = tx;
        Self {
            common_data: ExecuteTransactionCommon::L1(common_data),
            execute,
            received_timestamp_ms,
            raw_bytes: None,
        }
    }
}

impl TryFrom<Transaction> for L1Tx {
    type Error = &'static str;

    fn try_from(value: Transaction) -> Result<Self, Self::Error> {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
            ..
        } = value;
        match common_data {
            ExecuteTransactionCommon::L1(common_data) => Ok(L1Tx {
                execute,
                common_data,
                received_timestamp_ms,
            }),
            ExecuteTransactionCommon::L2(_) => Err("Cannot convert L2Tx to L1Tx"),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                Err("Cannot convert ProtocolUpgradeTx to L1Tx")
            }
        }
    }
}

impl L1Tx {
    pub fn serial_id(&self) -> PriorityOpId {
        self.common_data.serial_id
    }

    pub fn eth_block(&self) -> L1BlockNumber {
        L1BlockNumber(self.common_data.eth_block as u32)
    }

    pub fn hash(&self) -> H256 {
        self.common_data.hash()
    }
}

impl TryFrom<Log> for L1Tx {
    type Error = L1TxParseError;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        // TODO: refactor according to tx type
        let transaction_param_type = ParamType::Tuple(vec![
            ParamType::Uint(8),                                       // txType
            ParamType::Address,                                       // sender
            ParamType::Address,                                       // to
            ParamType::Uint(256),                                     // gasLimit
            ParamType::Uint(256),                                     // gasPerPubdataLimit
            ParamType::Uint(256),                                     // maxFeePerGas
            ParamType::Uint(256),                                     // maxPriorityFeePerGas
            ParamType::Address,                                       // paymaster
            ParamType::Uint(256),                                     // nonce (serial ID)
            ParamType::Uint(256),                                     // value
            ParamType::FixedArray(Box::new(ParamType::Uint(256)), 4), // reserved
            ParamType::Bytes,                                         // calldata
            ParamType::Bytes,                                         // signature
            ParamType::Array(Box::new(ParamType::Uint(256))),         // factory deps
            ParamType::Bytes,                                         // paymaster input
            ParamType::Bytes,                                         // reservedDynamic
        ]);

        let mut dec_ev = decode(
            &[
                ParamType::Uint(256),                         // tx ID
                ParamType::FixedBytes(32),                    // tx hash
                ParamType::Uint(64),                          // expiration block
                transaction_param_type,                       // transaction data
                ParamType::Array(Box::new(ParamType::Bytes)), // factory deps
            ],
            &event.data.0,
        )?;

        let eth_hash = event
            .transaction_hash
            .expect("Event transaction hash is missing");
        let eth_block = event
            .block_number
            .expect("Event block number is missing")
            .as_u64();

        let serial_id = PriorityOpId(
            dec_ev
                .remove(0)
                .into_uint()
                .as_ref()
                .map(U256::as_u64)
                .unwrap(),
        );

        let canonical_tx_hash = H256::from_slice(&dec_ev.remove(0).into_fixed_bytes().unwrap());

        let deadline_block = dec_ev.remove(0).into_uint().unwrap().as_u64();

        // Decoding transaction bytes
        let mut transaction = match dec_ev.remove(0) {
            Token::Tuple(tx) => tx,
            _ => unreachable!(),
        };

        assert_eq!(transaction.len(), 16);

        let tx_type = transaction.remove(0).into_uint().unwrap();
        assert_eq!(tx_type.clone(), U256::from(PRIORITY_OPERATION_L2_TX_TYPE));

        let sender = transaction.remove(0).into_address().unwrap();
        let contract_address = transaction.remove(0).into_address().unwrap();

        let gas_limit = transaction.remove(0).into_uint().unwrap();

        let gas_per_pubdata_limit = transaction.remove(0).into_uint().unwrap();

        let max_fee_per_gas = transaction.remove(0).into_uint().unwrap();

        let max_priority_fee_per_gas = transaction.remove(0).into_uint().unwrap();
        assert_eq!(max_priority_fee_per_gas, U256::zero());

        let paymaster = transaction.remove(0).into_address().unwrap();
        assert_eq!(paymaster, H160::zero());

        let serial_id_from_tx = transaction.remove(0).into_uint().unwrap();
        assert_eq!(serial_id_from_tx, serial_id.0.into()); // serial id from decoded from transaction bytes should be equal to one from event

        let msg_value = transaction.remove(0).into_uint().unwrap();

        let reserved = transaction
            .remove(0)
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(|token| token.into_uint().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reserved.len(), 4);

        let to_mint = reserved[0];
        let refund_recipient = u256_to_account_address(&reserved[1]);

        // All other reserved fields should be zero
        for item in reserved.iter().skip(2) {
            assert_eq!(item, &U256::zero());
        }

        let calldata = transaction.remove(0).into_bytes().unwrap();

        let signature = transaction.remove(0).into_bytes().unwrap();
        assert_eq!(signature.len(), 0);

        // TODO (SMA-1621): check that reservedDynamic are constructed correctly.
        let _factory_deps_hashes = transaction.remove(0).into_array().unwrap();
        let _paymaster_input = transaction.remove(0).into_bytes().unwrap();
        let _reserved_dynamic = transaction.remove(0).into_bytes().unwrap();

        // Decoding metadata

        // Finally, decode the factory dependencies
        let factory_deps = match dec_ev.remove(0) {
            Token::Array(factory_deps) => factory_deps,
            _ => unreachable!(),
        };

        let factory_deps = factory_deps
            .into_iter()
            .map(|token| token.into_bytes().unwrap())
            .collect::<Vec<_>>();

        let common_data = L1TxCommonData {
            serial_id,
            canonical_tx_hash,
            sender,
            deadline_block,
            layer_2_tip_fee: U256::zero(),
            to_mint,
            refund_recipient,
            full_fee: U256::zero(),
            gas_limit,
            max_fee_per_gas,
            gas_per_pubdata_limit,
            op_processing_type: OpProcessingType::Common,
            priority_queue_type: PriorityQueueType::Deque,
            eth_hash,
            eth_block,
        };

        let execute = Execute {
            contract_address,
            calldata: calldata.to_vec(),
            factory_deps: Some(factory_deps),
            value: msg_value,
        };
        Ok(Self {
            common_data,
            execute,
            received_timestamp_ms: unix_timestamp_ms(),
        })
    }
}

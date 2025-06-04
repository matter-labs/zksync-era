//! ZKsync types: essential type definitions for ZKsync network.
//!
//! `zksync_types` is a crate containing essential ZKsync network types, such as transactions, operations and
//! blockchain primitives.

#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::{fmt, ops::Range};

use anyhow::Context as _;
use fee::encoding_len;
pub use l1::L1TxCommonData;
pub use l2::L2TxCommonData;
pub use protocol_upgrade::{ProtocolUpgrade, ProtocolVersion};
use serde::{Deserialize, Serialize};
pub use storage::*;
pub use tx::Execute;
use zksync_basic_types::bytecode::BytecodeHash;
pub use zksync_basic_types::{protocol_version::ProtocolVersionId, vm, *};
pub use zksync_crypto_primitives::*;

use crate::{
    l2::{L2Tx, TransactionType},
    protocol_upgrade::ProtocolUpgradeTxCommonData,
};
pub use crate::{interop_root::InteropRoot, Nonce, H256, U256, U64};

pub type SerialId = u64;

pub mod abi;
pub mod aggregated_operations;
pub mod blob;
pub mod block;
pub mod commitment;
#[cfg(feature = "contract-verification")]
pub mod contract_verification;
pub mod debug_flat_call;
pub mod fee;
pub mod fee_model;
pub mod interop_root;
pub mod l1;
pub mod l2;
pub mod l2_to_l1_log;
pub mod priority_op_onchain_data;
pub mod protocol_upgrade;
pub mod snapshots;
pub mod storage;
pub mod system_contracts;
pub mod tokens;
pub mod tx;
pub mod zk_evm_types;

pub mod api;
pub mod base_token_ratio;
pub mod boojum_os;
pub mod eth_sender;
pub mod helpers;
#[cfg(feature = "protobuf")]
pub mod proto;
pub mod server_notification;
pub mod transaction_request;
pub mod utils;

/// Denotes the first byte of the special ZKsync's EIP-712-signed transaction.
pub const EIP_712_TX_TYPE: u8 = 0x71;

/// Denotes the first byte of the `EIP-1559` transaction.
pub const EIP_1559_TX_TYPE: u8 = 0x02;

/// Denotes the first byte of the `EIP-4844` transaction.
pub const EIP_4844_TX_TYPE: u8 = 0x03;

/// Denotes the first byte of the `EIP-2930` transaction.
pub const EIP_2930_TX_TYPE: u8 = 0x01;

/// Denotes the first byte of some legacy transaction, which type is unknown to the server.
pub const LEGACY_TX_TYPE: u8 = 0x0;

/// Denotes the first byte of the priority transaction.
pub const PRIORITY_OPERATION_L2_TX_TYPE: u8 = 0xff;

/// Denotes the first byte of the protocol upgrade transaction.
pub const PROTOCOL_UPGRADE_TX_TYPE: u8 = 0xfe;

#[derive(Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub common_data: ExecuteTransactionCommon,
    pub execute: Execute,
    pub received_timestamp_ms: u64,
    pub raw_bytes: Option<web3::Bytes>,
}

impl fmt::Debug for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(hash) = self.hash_for_debugging() {
            f.debug_tuple("Transaction").field(&hash).finish()
        } else {
            f.debug_struct("Transaction")
                .field("initiator_account", &self.initiator_account())
                .field("nonce", &self.nonce())
                .finish()
        }
    }
}

impl PartialEq for Transaction {
    fn eq(&self, other: &Transaction) -> bool {
        self.hash() == other.hash()
    }
}

impl Eq for Transaction {}

impl Transaction {
    /// Returns recipient account of the transaction.
    pub fn recipient_account(&self) -> Option<Address> {
        self.execute.contract_address
    }

    pub fn nonce(&self) -> Option<Nonce> {
        match &self.common_data {
            ExecuteTransactionCommon::L1(_) => None,
            ExecuteTransactionCommon::L2(tx) => Some(tx.nonce),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => None,
        }
    }

    pub fn is_l1(&self) -> bool {
        matches!(self.common_data, ExecuteTransactionCommon::L1(_))
    }

    pub fn tx_format(&self) -> TransactionType {
        match &self.common_data {
            ExecuteTransactionCommon::L1(tx) => tx.tx_format(),
            ExecuteTransactionCommon::L2(tx) => tx.transaction_type,
            ExecuteTransactionCommon::ProtocolUpgrade(tx) => tx.tx_format(),
        }
    }

    pub fn hash(&self) -> H256 {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.hash(),
            ExecuteTransactionCommon::L2(data) => data.hash(),
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.hash(),
        }
    }

    fn hash_for_debugging(&self) -> Option<H256> {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => Some(data.hash()),
            ExecuteTransactionCommon::L2(data) if data.input.is_some() => Some(data.hash()),
            ExecuteTransactionCommon::L2(_) => None,
            ExecuteTransactionCommon::ProtocolUpgrade(data) => Some(data.hash()),
        }
    }

    /// Returns the account that initiated this transaction.
    pub fn initiator_account(&self) -> Address {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.sender,
            ExecuteTransactionCommon::L2(data) => data.initiator_address,
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.sender,
        }
    }

    /// Returns the payer for L2 transaction and 0 for L1 transactions
    pub fn payer(&self) -> Address {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.sender,
            ExecuteTransactionCommon::L2(data) => {
                let paymaster = data.paymaster_params.paymaster;
                if paymaster == Address::default() {
                    data.initiator_address
                } else {
                    paymaster
                }
            }
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.sender,
        }
    }

    pub fn gas_limit(&self) -> U256 {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.gas_limit,
            ExecuteTransactionCommon::L2(data) => data.fee.gas_limit,
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.gas_limit,
        }
    }

    pub fn max_fee_per_gas(&self) -> U256 {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.max_fee_per_gas,
            ExecuteTransactionCommon::L2(data) => data.fee.max_fee_per_gas,
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.max_fee_per_gas,
        }
    }

    pub fn gas_per_pubdata_byte_limit(&self) -> U256 {
        match &self.common_data {
            ExecuteTransactionCommon::L1(data) => data.gas_per_pubdata_limit,
            ExecuteTransactionCommon::L2(data) => data.fee.gas_per_pubdata_limit,
            ExecuteTransactionCommon::ProtocolUpgrade(data) => data.gas_per_pubdata_limit,
        }
    }

    // Returns how many slots it takes to encode the transaction
    pub fn encoding_len(&self) -> usize {
        let data_len = self.execute.calldata.len();
        let factory_deps_len = self.execute.factory_deps.len();
        let (signature_len, paymaster_input_len) = match &self.common_data {
            ExecuteTransactionCommon::L1(_) => (0, 0),
            ExecuteTransactionCommon::L2(l2_common_data) => (
                l2_common_data.signature.len(),
                l2_common_data.paymaster_params.paymaster_input.len(),
            ),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => (0, 0),
        };

        encoding_len(
            data_len as u64,
            signature_len as u64,
            factory_deps_len as u64,
            paymaster_input_len as u64,
            0,
        )
    }
}

/// Optional input `Ethereum`-like encoded transaction if submitted via Web3 API.
///
/// If exists, its hash will be used to identify transaction.
/// Note, that for EIP712-type transactions, `hash` is not equal to the hash
/// of the `data`, but rather calculated by special formula.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputData {
    pub hash: H256,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecuteTransactionCommon {
    L1(L1TxCommonData),
    L2(L2TxCommonData),
    ProtocolUpgrade(ProtocolUpgradeTxCommonData),
}

impl fmt::Display for ExecuteTransactionCommon {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ExecuteTransactionCommon::L1(data) => write!(f, "L1TxCommonData: {:?}", data),
            ExecuteTransactionCommon::L2(data) => write!(f, "L2TxCommonData: {:?}", data),
            ExecuteTransactionCommon::ProtocolUpgrade(data) => {
                write!(f, "ProtocolUpgradeTxCommonData: {:?}", data)
            }
        }
    }
}

impl TryFrom<Transaction> for abi::Transaction {
    type Error = anyhow::Error;

    fn try_from(tx: Transaction) -> anyhow::Result<Self> {
        use ExecuteTransactionCommon as E;
        let factory_deps = tx.execute.factory_deps;
        Ok(match tx.common_data {
            E::L2(data) => Self::L2(
                data.input
                    .context("input is required for L2 transactions")?
                    .data,
            ),
            E::L1(data) => Self::L1 {
                tx: abi::L2CanonicalTransaction {
                    tx_type: PRIORITY_OPERATION_L2_TX_TYPE.into(),
                    from: address_to_u256(&data.sender),
                    to: address_to_u256(&tx.execute.contract_address.unwrap_or_default()),
                    gas_limit: data.gas_limit,
                    gas_per_pubdata_byte_limit: data.gas_per_pubdata_limit,
                    max_fee_per_gas: data.max_fee_per_gas,
                    max_priority_fee_per_gas: 0.into(),
                    paymaster: 0.into(),
                    nonce: data.serial_id.0.into(),
                    value: tx.execute.value,
                    reserved: [
                        data.to_mint,
                        address_to_u256(&data.refund_recipient),
                        0.into(),
                        0.into(),
                    ],
                    data: tx.execute.calldata,
                    signature: vec![],
                    factory_deps: factory_deps
                        .iter()
                        .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                        .collect(),
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                }
                .into(),
                factory_deps,
                eth_block: data.eth_block,
            },
            E::ProtocolUpgrade(data) => Self::L1 {
                tx: abi::L2CanonicalTransaction {
                    tx_type: PROTOCOL_UPGRADE_TX_TYPE.into(),
                    from: address_to_u256(&data.sender),
                    to: address_to_u256(&tx.execute.contract_address.unwrap_or_default()),
                    gas_limit: data.gas_limit,
                    gas_per_pubdata_byte_limit: data.gas_per_pubdata_limit,
                    max_fee_per_gas: data.max_fee_per_gas,
                    max_priority_fee_per_gas: 0.into(),
                    paymaster: 0.into(),
                    nonce: (data.upgrade_id as u16).into(),
                    value: tx.execute.value,
                    reserved: [
                        data.to_mint,
                        address_to_u256(&data.refund_recipient),
                        0.into(),
                        0.into(),
                    ],
                    data: tx.execute.calldata,
                    signature: vec![],
                    factory_deps: factory_deps
                        .iter()
                        .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                        .collect(),
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                }
                .into(),
                factory_deps,
                eth_block: data.eth_block,
            },
        })
    }
}

impl Transaction {
    /// Converts a transaction from its ABI representation.
    ///
    /// # Arguments
    ///
    /// - `allow_no_target` enables / disables L2 transactions without target (i.e., `to` field).
    ///   This field can only be absent for EVM deployment transactions.
    pub fn from_abi(tx: abi::Transaction, allow_no_target: bool) -> anyhow::Result<Self> {
        Ok(match tx {
            abi::Transaction::L1 {
                tx,
                factory_deps,
                eth_block,
            } => {
                let factory_deps_hashes: Vec<_> = factory_deps
                    .iter()
                    .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                    .collect();
                anyhow::ensure!(tx.factory_deps == factory_deps_hashes);
                for item in &tx.reserved[2..] {
                    anyhow::ensure!(item == &U256::zero());
                }
                assert_eq!(tx.max_priority_fee_per_gas, U256::zero());
                assert_eq!(tx.paymaster, U256::zero());
                assert!(tx.signature.is_empty());
                assert!(tx.paymaster_input.is_empty());
                assert!(tx.reserved_dynamic.is_empty());
                let hash = tx.hash();
                Transaction {
                    common_data: match tx.tx_type {
                        t if t == PRIORITY_OPERATION_L2_TX_TYPE.into() => {
                            ExecuteTransactionCommon::L1(L1TxCommonData {
                                serial_id: PriorityOpId(
                                    tx.nonce
                                        .try_into()
                                        .map_err(|err| anyhow::format_err!("{err}"))?,
                                ),
                                canonical_tx_hash: hash,
                                sender: u256_to_address(&tx.from),
                                layer_2_tip_fee: U256::zero(),
                                to_mint: tx.reserved[0],
                                refund_recipient: u256_to_address(&tx.reserved[1]),
                                full_fee: U256::zero(),
                                gas_limit: tx.gas_limit,
                                max_fee_per_gas: tx.max_fee_per_gas,
                                gas_per_pubdata_limit: tx.gas_per_pubdata_byte_limit,
                                op_processing_type: l1::OpProcessingType::Common,
                                priority_queue_type: l1::PriorityQueueType::Deque,
                                eth_block,
                            })
                        }
                        t if t == PROTOCOL_UPGRADE_TX_TYPE.into() => {
                            ExecuteTransactionCommon::ProtocolUpgrade(ProtocolUpgradeTxCommonData {
                                upgrade_id: tx.nonce.try_into().unwrap(),
                                canonical_tx_hash: hash,
                                sender: u256_to_address(&tx.from),
                                to_mint: tx.reserved[0],
                                refund_recipient: u256_to_address(&tx.reserved[1]),
                                gas_limit: tx.gas_limit,
                                max_fee_per_gas: tx.max_fee_per_gas,
                                gas_per_pubdata_limit: tx.gas_per_pubdata_byte_limit,
                                eth_block,
                            })
                        }
                        unknown_type => anyhow::bail!("unknown tx type {unknown_type}"),
                    },
                    execute: Execute {
                        contract_address: Some(u256_to_address(&tx.to)),
                        calldata: tx.data,
                        factory_deps,
                        value: tx.value,
                    },
                    raw_bytes: None,
                    received_timestamp_ms: helpers::unix_timestamp_ms(),
                }
            }
            abi::Transaction::L2(raw) => {
                let (req, hash) =
                    transaction_request::TransactionRequest::from_bytes_unverified(&raw)?;
                let mut tx = L2Tx::from_request_unverified(req, allow_no_target)?;
                tx.set_input(raw, hash);
                tx.into()
            }
        })
    }
}

#[derive(Clone, Serialize, Debug, Default, Eq, PartialEq, Hash)]
pub struct TransactionTimeRangeConstraint {
    pub timestamp_asserter_range: Option<Range<u64>>,
}

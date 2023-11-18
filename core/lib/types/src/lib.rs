//! zkSync types: essential type definitions for zkSync network.
//!
//! `zksync_types` is a crate containing essential zkSync network types, such as transactions, operations and
//! blockchain primitives.

#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use fee::encoding_len;
use serde::{Deserialize, Serialize};
use std::{fmt, fmt::Debug};

pub use crate::{Nonce, H256, U256, U64};

pub type SerialId = u64;

use crate::l2::TransactionType;
use crate::protocol_version::ProtocolUpgradeTxCommonData;
pub use event::{VmEvent, VmEventGroupKey};
pub use l1::L1TxCommonData;
pub use l2::L2TxCommonData;
pub use protocol_version::{ProtocolUpgrade, ProtocolVersion, ProtocolVersionId};
pub use storage::*;
pub use tx::primitives::*;
pub use tx::Execute;
pub use vm_version::VmVersion;
pub use zk_evm::{
    aux_structures::{LogQuery, Timestamp},
    reference_impls::event_sink::EventMessage,
    zkevm_opcode_defs::FarCallOpcode,
};

pub use zkevm_test_harness;
pub use zksync_basic_types::*;

pub mod aggregated_operations;
pub mod block;
pub mod circuit;
pub mod commitment;
pub mod contract_verification_api;
pub mod contracts;
pub mod event;
pub mod fee;
pub mod l1;
pub mod l2;
pub mod l2_to_l1_log;
pub mod priority_op_onchain_data;
pub mod protocol_version;
pub mod storage;
pub mod storage_writes_deduplicator;
pub mod system_contracts;
pub mod tokens;
pub mod tx;
pub mod vm_trace;

pub mod api;
pub mod eth_sender;
pub mod helpers;
pub mod proofs;
pub mod prover_server_api;
pub mod transaction_request;
pub mod utils;
pub mod vk_transform;
pub mod vm_version;

mod proto;

/// Denotes the first byte of the special zkSync's EIP-712-signed transaction.
pub const EIP_712_TX_TYPE: u8 = 0x71;

/// Denotes the first byte of the `EIP-1559` transaction.
pub const EIP_1559_TX_TYPE: u8 = 0x02;

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
    pub raw_bytes: Option<Bytes>,
}

impl std::fmt::Debug for Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Transaction").field(&self.hash()).finish()
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
    pub fn recipient_account(&self) -> Address {
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
        let factory_deps_len = self
            .execute
            .factory_deps
            .as_ref()
            .map(|deps| deps.len())
            .unwrap_or_default();
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
/// If exists, its hash will be used to identify transaction.
/// Note, that for EIP712-type transactions, `hash` is not equal to the hash
/// of the `data`, but rather calculated by special formula.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputData {
    pub hash: H256,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

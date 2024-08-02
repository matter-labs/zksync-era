use serde::{Deserialize, Serialize};

use crate::{aggregated_operations::AggregatedActionType, Address, Nonce, H256};

/// A forward-compatible `enum` describing a EIP4844 sidecar
///
/// This enum in `bincode`-encoded form is stored in the database
/// alongside all other transaction-related fields for EIP4844 transactions
/// in `eth_txs` and `eth_tx_history` tables.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum EthTxBlobSidecar {
    EthTxBlobSidecarV1(EthTxBlobSidecarV1),
}

impl From<EthTxBlobSidecarV1> for EthTxBlobSidecar {
    fn from(value: EthTxBlobSidecarV1) -> Self {
        Self::EthTxBlobSidecarV1(value)
    }
}

/// All sidecar data for a single blob for the EIP4844 transaction.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SidecarBlobV1 {
    /// Blob itself
    pub blob: Vec<u8>,
    /// Blob commitment
    pub commitment: Vec<u8>,
    /// Blob proof
    pub proof: Vec<u8>,
    /// Blob commitment versioned hash
    pub versioned_hash: Vec<u8>,
}

/// A first version of sidecars for blob transactions as they are described in EIP4844.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EthTxBlobSidecarV1 {
    /// A vector of blobs for this tx and their commitments and proofs.
    pub blobs: Vec<SidecarBlobV1>,
}

#[derive(Clone)]
pub struct EthTx {
    pub id: u32,
    pub nonce: Nonce,
    pub contract_address: Address,
    pub raw_tx: Vec<u8>,
    pub tx_type: AggregatedActionType,
    pub created_at_timestamp: u64,
    /// If this field is `Some` then it contains address of a custom operator that has sent
    /// this transaction. If it is set to `None` this transaction was sent by the main operator.
    pub from_addr: Option<Address>,
    pub blob_sidecar: Option<EthTxBlobSidecar>,
}

impl std::fmt::Debug for EthTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Do not print `raw_tx`
        f.debug_struct("EthTx")
            .field("id", &self.id)
            .field("nonce", &self.nonce)
            .field("contract_address", &self.contract_address)
            .field("tx_type", &self.tx_type)
            .field("created_at_timestamp", &self.created_at_timestamp)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct TxHistory {
    pub id: u32,
    pub eth_tx_id: u32,
    pub base_fee_per_gas: u64,
    pub priority_fee_per_gas: u64,
    pub blob_base_fee_per_gas: Option<u64>,
    pub tx_hash: H256,
    pub signed_raw_tx: Vec<u8>,
    pub sent_at_block: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct TxHistoryToSend {
    pub id: u32,
    pub eth_tx_id: u32,
    pub base_fee_per_gas: u64,
    pub priority_fee_per_gas: u64,
    pub tx_hash: H256,
    pub signed_raw_tx: Vec<u8>,
    pub nonce: Nonce,
}

use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BlockNumber, SLChainId};

use crate::{aggregated_operations::AggregatedActionType, Address, Nonce, H256};

/// A forward-compatible `enum` describing a EIP4844 sidecar
///
/// This enum in `bincode`-encoded form is stored in the database
/// alongside all other transaction-related fields for EIP4844 transactions
/// in `eth_txs` and `eth_tx_history` tables.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum EthTxBlobSidecar {
    EthTxBlobSidecarV1(EthTxBlobSidecarV1),
    EthTxBlobSidecarV2(EthTxBlobSidecarV2),
}

impl From<EthTxBlobSidecarV1> for EthTxBlobSidecar {
    fn from(value: EthTxBlobSidecarV1) -> Self {
        Self::EthTxBlobSidecarV1(value)
    }
}

impl From<EthTxBlobSidecarV2> for EthTxBlobSidecar {
    fn from(value: EthTxBlobSidecarV2) -> Self {
        Self::EthTxBlobSidecarV2(value)
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
    /// Cell proofs
    pub cell_proofs: Option<Vec<Vec<u8>>>,
    /// Blob commitment versioned hash
    pub versioned_hash: Vec<u8>,
}

/// All sidecar data for a single blob for the EIP7594 transaction.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SidecarBlobV2 {
    /// Blob itself
    pub blob: Vec<u8>,
    /// Blob commitment
    pub commitment: Vec<u8>,
    /// Blob commitment versioned hash
    pub versioned_hash: Vec<u8>,
    /// The KZG proofs for each of the cells in the blob.
    pub cell_proofs: Vec<Vec<u8>>,
}

/// A first version of sidecars for blob transactions as they are described in EIP4844.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EthTxBlobSidecarV1 {
    /// A vector of blobs for this tx and their commitments and proofs.
    pub blobs: Vec<SidecarBlobV1>,
}

/// A first version of sidecars for blob transactions as they are described in EIP4844.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EthTxBlobSidecarV2 {
    /// A vector of blobs for this tx and their commitments and proofs.
    pub blobs: Vec<SidecarBlobV2>,
}

#[derive(Clone)]
pub struct EthTx {
    pub id: u32,
    pub nonce: Nonce,
    pub contract_address: Address,
    pub raw_tx: Vec<u8>,
    pub tx_type: AggregatedActionType,
    pub created_at_timestamp: u64,
    pub predicted_gas_cost: Option<u64>,
    /// If this field is `Some` then it contains address of a custom operator that has sent
    /// this transaction. If it is set to `None` this transaction was sent by the main operator.
    pub from_addr: Option<Address>,
    pub blob_sidecar: Option<EthTxBlobSidecar>,
    pub is_gateway: bool,
    pub chain_id: Option<SLChainId>,
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
            .field("predicted_gas_cost", &self.predicted_gas_cost)
            .field("chain_id", &self.chain_id)
            .field("is_gateway", &self.is_gateway)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct TxHistory {
    pub id: u32,
    pub eth_tx_id: u32,
    pub chain_id: Option<SLChainId>,
    pub tx_type: AggregatedActionType,
    pub base_fee_per_gas: u64,
    pub priority_fee_per_gas: u64,
    pub blob_base_fee_per_gas: Option<u64>,
    pub tx_hash: H256,
    pub signed_raw_tx: Vec<u8>,
    pub sent_at_block: Option<u32>,
    pub max_gas_per_pubdata: Option<u64>,
    pub eth_tx_finality_status: EthTxFinalityStatus,
    pub sent_successfully: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EthTxFinalityStatus {
    Pending,
    FastFinalized,
    Finalized,
}

impl FromStr for EthTxFinalityStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fast_finalized" => Ok(Self::FastFinalized),
            "finalized" => Ok(Self::Finalized),
            "pending" => Ok(Self::Pending),
            _ => Err("Incorrect finality status"),
        }
    }
}

impl Display for EthTxFinalityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FastFinalized => write!(f, "fast_finalized"),
            Self::Finalized => write!(f, "finalized"),
            Self::Pending => write!(f, "pending"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct L1BlockNumbers {
    pub fast_finality: L1BlockNumber,
    pub finalized: L1BlockNumber,
    pub latest: L1BlockNumber,
}

impl L1BlockNumbers {
    pub fn get_finality_status_for_block(&self, block_number: u32) -> EthTxFinalityStatus {
        if block_number <= self.finalized.0 {
            EthTxFinalityStatus::Finalized
        } else if block_number <= self.fast_finality.0 {
            EthTxFinalityStatus::FastFinalized
        } else {
            EthTxFinalityStatus::Pending
        }
    }

    /// returns new finality status if finality status changed
    pub fn get_finality_update(
        &self,
        current_status: EthTxFinalityStatus,
        included_at_block: u32,
    ) -> Option<EthTxFinalityStatus> {
        let finality_status = self.get_finality_status_for_block(included_at_block);
        if finality_status == current_status {
            return None;
        }
        Some(finality_status)
    }
}

//! API types related to the External Node specific methods.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zk_evm::ethereum_types::Address;
use zksync_basic_types::{L1BatchNumber, MiniblockNumber, H256};
use zksync_contracts::BaseSystemContractsHashes;

/// Representation of the L2 block, as needed for the EN synchronization.
/// This structure has several fields that describe *L1 batch* rather than
/// *L2 block*, thus they are the same for all the L2 blocks in the batch.
///
/// This is done to simplify the implementation of the synchronization protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBlock {
    /// Number of the L2 block.
    pub number: MiniblockNumber,
    /// Number of L1 batch this L2 block belongs to.
    pub l1_batch_number: L1BatchNumber,
    /// Whether this L2 block is the last in the L1 batch.
    /// Currently, should always indicate the fictive miniblock.
    pub last_in_batch: bool,
    /// L2 block timestamp.
    pub timestamp: u64,
    /// Hash of the L2 block (not the Merkle root hash).
    pub root_hash: Option<H256>,
    /// Hash of the block's commit transaction on L1.
    /// May be `None` if the corresponsing L1 batch is not committed yet.
    pub commit_tx_hash: Option<H256>,
    /// Timestamp of the commit transaction, as provided by the main node.
    pub committed_at: Option<DateTime<Utc>>,
    /// Hash of the block's prove transaction on L1.
    /// May be `None` if the corresponsing L1 batch is not proven yet.
    pub prove_tx_hash: Option<H256>,
    /// Timestamp of the prove transaction, as provided by the main node.
    pub proven_at: Option<DateTime<Utc>>,
    /// Hash of the block's execute transaction on L1.
    /// May be `None` if the corresponsing L1 batch is not executed yet.
    pub execute_tx_hash: Option<H256>,
    /// Timestamp of the execute transaction, as provided by the main node.
    pub executed_at: Option<DateTime<Utc>>,
    /// L1 gas price used as VM parameter for the L1 batch corresponding to this L2 block.
    pub l1_gas_price: u64,
    /// L2 gas price used as VM parameter for the L1 batch corresponding to this L2 block.
    pub l2_fair_gas_price: u64,
    /// Hashes of the base system contracts used in for the L1 batch corresponding to this L2 block.
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    /// Address of the operator account who produced for the L1 batch corresponding to this L2 block.
    pub operator_address: Address,
    /// List of transactions included in the L2 block.
    /// These are not the API representation of transactions, but rather the actual type used by the server.
    /// May be `None` if transactions were not requested (as opposed to the empty vector).
    pub transactions: Option<Vec<crate::Transaction>>,
}

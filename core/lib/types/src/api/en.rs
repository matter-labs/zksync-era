//! API types related to the External Node specific methods.

use serde::{Deserialize, Serialize};
use zk_evm::ethereum_types::Address;
use zksync_basic_types::{L1BatchNumber, MiniblockNumber, H256};
use zksync_contracts::BaseSystemContractsHashes;

use crate::{block::ConsensusBlockFields, ProtocolVersionId};

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
    /// Number of virtual blocks associated with this L2 block.
    pub virtual_blocks: Option<u32>,
    /// Hash of the L2 block.
    pub hash: Option<H256>,
    /// Version of the protocol used for this block.
    pub protocol_version: ProtocolVersionId,
    /// Consensus-related information about the block. Not present if consensus is not enabled
    /// for the environment.
    pub consensus: Option<ConsensusBlockFields>,
}

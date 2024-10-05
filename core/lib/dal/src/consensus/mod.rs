use std::collections::BTreeMap;

use zksync_concurrency::net;
use zksync_consensus_roles::{attester, node, validator};
use zksync_types::{
    ethabi,
    block::{rolling_txs_hash},
    Address, L1BatchNumber,
    ProtocolVersionId, Transaction, H256,
};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;

pub mod proto;
mod conv;
#[cfg(test)]
mod testonly;
#[cfg(test)]
mod tests;

/// Global config of the consensus.
#[derive(Debug, PartialEq, Clone)]
pub struct GlobalConfig {
    pub genesis: validator::Genesis,
    pub registry_address: Option<ethabi::Address>,
    pub seed_peers: BTreeMap<node::PublicKey, net::Host>,
}

/// Global attestation status served by
/// `attestationStatus` RPC.
#[derive(Debug, PartialEq, Clone)]
pub struct AttestationStatus {
    pub genesis: validator::GenesisHash,
    pub next_batch_to_attest: attester::BatchNumber,
}

/// Block metadata that should have been committed to on L1,
/// but is currently not, and is served by the main node instead.
#[derive(Debug,PartialEq,Clone)]
pub struct BlockMetadata {
    pub batch_number: attester::BatchNumber,
    pub protocol_version: ProtocolVersionId,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub virtual_blocks: u32,
    pub operator_address: Address,
}

#[derive(Debug,PartialEq,Clone)]
pub struct BlockDigest {
    pub txs_rolling_hash: H256,
    pub timestamp: u64,
}

impl BlockDigest {
    pub fn from_payload(payload: &Payload) -> Self {
        Self {
            txs_rolling_hash: rolling_txs_hash(&payload.transactions),
            timestamp: payload.timestamp,
        }
    }
}

#[derive(Debug,PartialEq,Clone)]
pub struct StorageProof {
    pub value: H256,
    pub index: u64,
    pub merkle_path: Vec<H256>,
}

/// Commitment to the blocks of an L1 batch.
/// It can be verified against the L1 contract.
/// It currently doesn't contain enough data to verify
/// all the data in the blocks (see `BlockMetadata`).
#[derive(Debug,PartialEq)]
pub struct BatchProof {
    pub info: StoredBatchInfo,
    pub current_l2_block_info: StorageProof,
    pub tx_rolling_hash: StorageProof,
    pub l2_block_hash_entry: StorageProof,
    
    pub initial_hash: H256,
    pub blocks: Vec<BlockDigest>,
}

impl BatchProof {
    /// Batch number.
    pub fn batch_number(&self) -> attester::BatchNumber {
        attester::BatchNumber(self.info.batch_number)
    }
   
    /// Decodes the batch proof from justification.
    pub fn decode(justification: &validator::Justification) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&justification.0)
    }

    /// Encodes the batch proof into justification.
    pub fn encode(&self) -> validator::Justification {
        validator::Justification(zksync_protobuf::encode(self))
    }
}

/// L2 block (= miniblock) payload.
#[derive(Debug, PartialEq)]
pub struct Payload {
    pub protocol_version: ProtocolVersionId,
    pub hash: H256,
    pub l1_batch_number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub fair_pubdata_price: Option<u64>,
    pub virtual_blocks: u32,
    pub operator_address: Address,
    pub transactions: Vec<Transaction>,
    pub last_in_batch: bool,
}

impl Payload {
    pub fn decode(payload: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&payload.0)
    }

    pub fn encode(&self) -> validator::Payload {
        validator::Payload(zksync_protobuf::encode(self))
    }
}

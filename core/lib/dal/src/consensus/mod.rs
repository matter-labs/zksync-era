use std::collections::BTreeMap;

use zksync_concurrency::net;
use zksync_consensus_roles::{attester, node, validator};
use zksync_types::{
    commitment::PubdataParams, ethabi, Address, L1BatchNumber, ProtocolVersionId, Transaction, H256,
};

mod conv;
pub mod proto;
#[cfg(test)]
mod testonly;
#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Clone)]
pub struct BlockMetadata {
    pub payload_hash: validator::PayloadHash,
}

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
    pub pubdata_params: PubdataParams,
}

impl Payload {
    pub fn decode(payload: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&payload.0)
    }

    pub fn encode(&self) -> validator::Payload {
        validator::Payload(zksync_protobuf::encode(self))
    }
}

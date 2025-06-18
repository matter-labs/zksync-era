use std::collections::BTreeMap;

use zksync_concurrency::net;
use zksync_consensus_engine::Last;
use zksync_consensus_roles::{node, validator};
use zksync_types::{
    commitment::PubdataParams, ethabi, Address, InteropRoot, L1BatchNumber, ProtocolVersionId,
    Transaction, H256,
};

mod conv;
pub mod proto;
#[cfg(test)]
mod testonly;
#[cfg(test)]
mod tests;

/// Block certificate.
#[derive(Debug, PartialEq, Clone)]
pub enum BlockCertificate {
    V1(validator::v1::CommitQC),
    V2(validator::v2::CommitQC),
}

impl BlockCertificate {
    /// Returns block number.
    pub fn number(&self) -> validator::BlockNumber {
        match self {
            Self::V1(qc) => qc.message.proposal.number,
            Self::V2(qc) => qc.message.proposal.number,
        }
    }

    /// Returns payload hash.
    pub fn payload_hash(&self) -> validator::PayloadHash {
        match self {
            Self::V1(qc) => qc.message.proposal.payload,
            Self::V2(qc) => qc.message.proposal.payload,
        }
    }
}

impl From<BlockCertificate> for Last {
    fn from(cert: BlockCertificate) -> Self {
        match cert {
            BlockCertificate::V1(qc) => Last::FinalV1(qc),
            BlockCertificate::V2(qc) => Last::FinalV2(qc),
        }
    }
}

/// Block metadata.
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
    pub interop_roots: Vec<InteropRoot>,
}

impl Payload {
    pub fn decode(payload: &validator::Payload) -> anyhow::Result<Self> {
        zksync_protobuf::decode(&payload.0)
    }

    pub fn encode(&self) -> validator::Payload {
        dbg!(self);
        validator::Payload(zksync_protobuf::encode(self))
    }
}

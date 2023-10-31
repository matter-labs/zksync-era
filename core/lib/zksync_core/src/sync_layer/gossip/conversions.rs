//! Conversion logic between server and consensus types.

use anyhow::Context as _;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};

use zksync_consensus_roles::validator::{
    BlockHeader, BlockNumber, CommitQC, FinalBlock, Payload, ReplicaCommit, ViewNumber,
    CURRENT_VERSION,
};
use zksync_types::{
    api::en::SyncBlock, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256,
};

use crate::sync_layer::fetcher::FetchedBlock;

// FIXME: should use Protobuf
#[derive(Debug, Serialize, Deserialize)]
struct BlockPayload {
    hash: H256,
    l1_batch_number: L1BatchNumber,
    timestamp: u64,
    l1_gas_price: u64,
    l2_fair_gas_price: u64,
    virtual_blocks: u32,
    operator_address: Address,
    transactions: Vec<zksync_types::Transaction>,
}

pub(super) fn sync_block_to_consensus_block(block: SyncBlock) -> FinalBlock {
    let payload = serde_json::to_vec(&BlockPayload {
        hash: block.hash.unwrap_or_default(),
        l1_batch_number: block.l1_batch_number,
        timestamp: block.timestamp,
        l1_gas_price: block.l1_gas_price,
        l2_fair_gas_price: block.l2_fair_gas_price,
        virtual_blocks: block.virtual_blocks.unwrap_or(0),
        operator_address: block.operator_address,
        transactions: block
            .transactions
            .expect("Transactions are always requested"),
    });
    let payload = Payload(payload.expect("Failed serializing block payload"));
    let header = BlockHeader {
        parent: thread_rng().gen(), // FIXME
        number: BlockNumber(block.number.0.into()),
        payload: payload.hash(),
    };
    FinalBlock {
        header,
        payload,
        justification: CommitQC {
            message: ReplicaCommit {
                protocol_version: CURRENT_VERSION,
                view: ViewNumber(header.number.0),
                proposal: header,
            },
            ..thread_rng().gen() // FIXME
        },
    }
}

impl FetchedBlock {
    pub(super) fn from_gossip_block(block: &FinalBlock) -> anyhow::Result<Self> {
        let number = u32::try_from(block.header.number.0)
            .context("Integer overflow converting block number")?;
        let payload: BlockPayload = serde_json::from_slice(&block.payload.0)
            .context("Failed deserializing block payload")?;

        Ok(Self {
            number: MiniblockNumber(number),
            l1_batch_number: payload.l1_batch_number,
            protocol_version: ProtocolVersionId::latest(), // FIXME
            timestamp: payload.timestamp,
            hash: payload.hash,
            l1_gas_price: payload.l1_gas_price,
            l2_fair_gas_price: payload.l2_fair_gas_price,
            virtual_blocks: payload.virtual_blocks,
            operator_address: payload.operator_address,
            transactions: payload.transactions,
        })
    }
}

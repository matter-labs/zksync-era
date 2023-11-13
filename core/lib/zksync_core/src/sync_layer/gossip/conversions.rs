//! Conversion logic between server and consensus types.

use anyhow::Context as _;

use zksync_consensus_roles::validator::{
    BlockHeader, BlockHeaderHash, BlockNumber, CommitQC, FinalBlock,
};
use zksync_types::{
    api::en::SyncBlock,
    block::{CommitQCBytes, ConsensusBlockFields},
    MiniblockNumber, ProtocolVersionId, H256,
};

use crate::sync_layer::fetcher::FetchedBlock;

pub(super) fn sync_block_to_consensus_block(mut block: SyncBlock) -> anyhow::Result<FinalBlock> {
    let number = BlockNumber(block.number.0.into());
    let consensus = block.consensus.take().context("Missing consensus fields")?;
    let prev_block_hash = consensus.prev_block_hash;
    let payload = crate::consensus::payload::sync_block_to_payload(block);
    let header = BlockHeader {
        parent: BlockHeaderHash::from_bytes(prev_block_hash.0),
        number,
        payload: payload.hash(),
    };
    let justification: CommitQC =
        zksync_consensus_schema::decode(consensus.commit_qc_bytes.as_ref())
            .context("Failed deserializing commit QC from Protobuf")?;
    Ok(FinalBlock {
        header,
        payload,
        justification,
    })
}

impl FetchedBlock {
    pub(super) fn from_gossip_block(
        block: &FinalBlock,
        last_in_batch: bool,
    ) -> anyhow::Result<Self> {
        let number = u32::try_from(block.header.number.0)
            .context("Integer overflow converting block number")?;
        let payload: crate::consensus::payload::Payload = serde_json::from_slice(&block.payload.0)
            .context("Failed deserializing block payload")?;

        Ok(Self {
            number: MiniblockNumber(number),
            l1_batch_number: payload.l1_batch_number,
            last_in_batch,
            protocol_version: ProtocolVersionId::latest(), // FIXME
            timestamp: payload.timestamp,
            hash: payload.hash,
            l1_gas_price: payload.l1_gas_price,
            l2_fair_gas_price: payload.l2_fair_gas_price,
            virtual_blocks: payload.virtual_blocks,
            operator_address: payload.operator_address,
            transactions: payload.transactions,
            consensus: Some(ConsensusBlockFields {
                prev_block_hash: H256(*block.header.parent.as_bytes()),
                commit_qc_bytes: CommitQCBytes::new(zksync_consensus_schema::canonical(
                    &block.justification,
                )),
            }),
        })
    }
}

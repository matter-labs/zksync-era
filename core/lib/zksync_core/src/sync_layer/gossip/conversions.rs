//! Conversion logic between server and consensus types.
use anyhow::Context as _;
use zksync_consensus_roles::validator::FinalBlock;
use zksync_dal::blocks_dal::ConsensusBlockFields;
use zksync_types::MiniblockNumber;

use crate::{consensus, sync_layer::fetcher::FetchedBlock};

impl FetchedBlock {
    pub(super) fn from_gossip_block(
        block: &FinalBlock,
        last_in_batch: bool,
    ) -> anyhow::Result<Self> {
        let number = u32::try_from(block.header.number.0)
            .context("Integer overflow converting block number")?;
        let payload = consensus::Payload::decode(&block.payload)
            .context("Failed deserializing block payload")?;

        Ok(Self {
            number: MiniblockNumber(number),
            l1_batch_number: payload.l1_batch_number,
            last_in_batch,
            protocol_version: payload.protocol_version,
            timestamp: payload.timestamp,
            reference_hash: Some(payload.hash),
            l1_gas_price: payload.l1_gas_price,
            l2_fair_gas_price: payload.l2_fair_gas_price,
            fair_pubdata_price: payload.fair_pubdata_price,
            virtual_blocks: payload.virtual_blocks,
            operator_address: payload.operator_address,
            transactions: payload.transactions,
            consensus: Some(ConsensusBlockFields {
                parent: block.header.parent,
                justification: block.justification.clone(),
            }),
        })
    }
}

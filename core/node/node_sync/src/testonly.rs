use std::collections::HashMap;

use zksync_config::GenesisConfig;
use zksync_eth_client::EnrichedClientError;
use zksync_node_genesis::mock_genesis_config;
use zksync_types::{api, Address, L2BlockNumber, ProtocolVersionId, H256};
use zksync_web3_decl::error::EnrichedClientResult;

use super::MainNodeClient;

#[derive(Debug, Default)]
pub struct MockMainNodeClient {
    pub l2_blocks: Vec<api::en::SyncBlock>,
    pub block_number_offset: u32,
    pub protocol_versions: HashMap<u16, api::ProtocolVersionInfo>,
    pub system_contracts: HashMap<H256, Vec<u8>>,
}

#[async_trait::async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        Ok(self.system_contracts.get(&hash).cloned())
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        _address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        Err(EnrichedClientError::custom(
            "not implemented",
            "fetch_genesis_contract_bytecode",
        ))
    }

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersionInfo>> {
        let protocol_version = protocol_version as u16;
        Ok(self.protocol_versions.get(&protocol_version).cloned())
    }

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        if let Some(number) = self.l2_blocks.len().checked_sub(1) {
            Ok(L2BlockNumber(number as u32))
        } else {
            Err(EnrichedClientError::custom(
                "not implemented",
                "fetch_l2_block_number",
            ))
        }
    }

    async fn fetch_l2_block(
        &self,
        number: L2BlockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<api::en::SyncBlock>> {
        let Some(block_index) = number.0.checked_sub(self.block_number_offset) else {
            return Ok(None);
        };
        let Some(mut block) = self.l2_blocks.get(block_index as usize).cloned() else {
            return Ok(None);
        };
        if !with_transactions {
            block.transactions = None;
        }
        Ok(Some(block))
    }

    async fn fetch_genesis_config(&self) -> EnrichedClientResult<GenesisConfig> {
        Ok(mock_genesis_config())
    }
}

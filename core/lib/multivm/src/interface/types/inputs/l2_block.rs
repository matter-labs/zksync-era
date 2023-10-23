use zksync_types::{block::MiniblockExecutionData, H256};

#[derive(Debug, Clone, Copy)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
}

impl L2BlockEnv {
    pub fn from_miniblock_data(miniblock_execution_data: &MiniblockExecutionData) -> Self {
        Self {
            number: miniblock_execution_data.number.0,
            timestamp: miniblock_execution_data.timestamp,
            prev_block_hash: miniblock_execution_data.prev_block_hash,
            max_virtual_blocks_to_create: miniblock_execution_data.virtual_blocks,
        }
    }
}

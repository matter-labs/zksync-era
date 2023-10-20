use zksync_types::{block::MiniblockExecutionData, H256};

#[derive(Debug, Clone, Copy)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
}

impl From<&MiniblockExecutionData> for L2BlockEnv {
    fn from(miniblock_execution_data: &MiniblockExecutionData) -> Self {
        Self {
            number: miniblock_execution_data.number.0,
            timestamp: miniblock_execution_data.timestamp,
            prev_block_hash: miniblock_execution_data.prev_block_hash,
            max_virtual_blocks_to_create: miniblock_execution_data.virtual_blocks,
        }
    }
}

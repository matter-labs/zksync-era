use serde::{Deserialize, Serialize};
use zksync_types::{block::L2BlockExecutionData, H256};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
}

impl L2BlockEnv {
    pub fn from_l2_block_data(miniblock_execution_data: &L2BlockExecutionData) -> Self {
        Self {
            number: miniblock_execution_data.number.0,
            timestamp: miniblock_execution_data.timestamp,
            prev_block_hash: miniblock_execution_data.prev_block_hash,
            max_virtual_blocks_to_create: miniblock_execution_data.virtual_blocks,
        }
    }
}

/// Pending block information used in oneshot transaction / call execution.
// FIXME: come up with a better name
#[derive(Debug, Clone, Copy)]
pub struct PendingL2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub txs_rolling_hash: H256,
}

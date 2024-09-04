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
    pub fn from_l2_block_data(execution_data: &L2BlockExecutionData) -> Self {
        Self {
            number: execution_data.number.0,
            timestamp: execution_data.timestamp,
            prev_block_hash: execution_data.prev_block_hash,
            max_virtual_blocks_to_create: execution_data.virtual_blocks,
        }
    }
}

/// Current block information stored in the system context contract. Can be used to set up
/// oneshot transaction / call execution.
#[derive(Debug, Clone, Copy)]
pub struct StoredL2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub txs_rolling_hash: H256,
}

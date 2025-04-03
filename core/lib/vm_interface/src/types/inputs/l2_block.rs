use serde::{Deserialize, Serialize};
use zksync_types::{block::L2BlockExecutionData, MessageRoot, H256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
    pub msg_roots: Vec<MessageRoot>,
}

impl L2BlockEnv {
    pub fn from_l2_block_data(execution_data: &L2BlockExecutionData) -> Self {
        Self {
            number: execution_data.number.0,
            timestamp: execution_data.timestamp,
            prev_block_hash: execution_data.prev_block_hash,
            max_virtual_blocks_to_create: execution_data.virtual_blocks,
            msg_roots: execution_data.msg_roots.clone(),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            number: self.number,
            timestamp: self.timestamp,
            prev_block_hash: self.prev_block_hash,
            max_virtual_blocks_to_create: self.max_virtual_blocks_to_create,
            msg_roots: self.msg_roots.clone(),
        }
    }
}

/// Current block information stored in the system context contract. Can be used to set up
/// oneshot transaction / call execution.
#[derive(Debug, Clone)]
pub struct StoredL2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub txs_rolling_hash: H256,
    pub msg_roots: Vec<MessageRoot>,
}

impl StoredL2BlockEnv {
    pub fn clone(&self) -> Self {
        Self {
            number: self.number,
            timestamp: self.timestamp,
            txs_rolling_hash: self.txs_rolling_hash,
            msg_roots: self.msg_roots.clone(),
        }
    }
}

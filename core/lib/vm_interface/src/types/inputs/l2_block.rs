use serde::{Deserialize, Serialize};
use zksync_types::{block::L2BlockExecutionData, InteropRoot, H256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
    pub interop_roots: Vec<InteropRoot>,
}

impl L2BlockEnv {
    pub fn from_l2_block_data(execution_data: &L2BlockExecutionData) -> Self {
        Self {
            number: execution_data.number.0,
            timestamp: execution_data.timestamp,
            prev_block_hash: execution_data.prev_block_hash,
            max_virtual_blocks_to_create: execution_data.virtual_blocks,
            interop_roots: execution_data.interop_roots.clone(),
        }
    }
}

/// Current block information stored in the system context contract. Can be used to set up
/// oneshot transaction / call execution.
#[derive(Debug)]
pub struct StoredL2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub txs_rolling_hash: H256,
    pub interop_roots: Vec<InteropRoot>,
}

impl Clone for StoredL2BlockEnv {
    fn clone(&self) -> Self {
        Self {
            number: self.number,
            timestamp: self.timestamp,
            txs_rolling_hash: self.txs_rolling_hash,
            interop_roots: self.interop_roots.clone(),
        }
    }
}

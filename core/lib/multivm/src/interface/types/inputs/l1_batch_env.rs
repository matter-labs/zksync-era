use zksync_types::{fee_model::BatchFeeInput, Address, L1BatchNumber, H256};

use super::L2BlockEnv;

/// Unique params for each block
#[derive(Debug, Clone)]
pub struct L1BatchEnv {
    // If previous batch hash is None, then this is the first batch
    pub previous_batch_hash: Option<H256>,
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub fee_input: BatchFeeInput,
    pub fee_account: Address,
    pub enforced_base_fee: Option<u64>,
    pub first_l2_block: L2BlockEnv,
}

use crate::types::inputs::L2BlockEnv;
use zksync_types::{Address, L1BatchNumber, H256, U256};

/// Unique params for each block
#[derive(Debug, Clone)]
pub struct L1BatchEnv {
    // If previous batch hash is None, then this is the first batch
    pub previous_batch_hash: Option<H256>,
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_gas_price: u64,
    pub fair_l2_gas_price: u64,
    pub fee_account: Address,
    pub enforced_base_fee: Option<u64>,
    pub first_l2_block: L2BlockEnv,
}

use super::L2BlockEnv;
use zksync_types::{Address, L1BatchNumber, H256};

use crate::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;

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

impl L1BatchEnv {
    pub fn base_fee(&self) -> u64 {
        if let Some(base_fee) = self.enforced_base_fee {
            return base_fee;
        }
        let (base_fee, _) =
            derive_base_fee_and_gas_per_pubdata(self.l1_gas_price, self.fair_l2_gas_price);
        base_fee
    }
    pub(crate) fn block_gas_price_per_pubdata(&self) -> u64 {
        derive_base_fee_and_gas_per_pubdata(self.l1_gas_price, self.fair_l2_gas_price).1
    }
}

use zksync_types::{
    fee_model::BatchFeeModelInput, Address, L1BatchNumber, ProtocolVersionId, VmVersion, H256, U256,
};

use super::{L2BlockEnv, SystemEnv};
use crate::utils::derive_base_fee_and_gas_per_pubdata;

/// Unique params for each batch
#[derive(Debug, Clone)]
pub struct L1BatchEnv {
    // If previous batch hash is None, then this is the first batch
    pub previous_batch_hash: Option<H256>,
    pub number: L1BatchNumber,
    pub timestamp: u64,

    pub fee_input: BatchFeeModelInput,
    // /// The price of the L1 gas in Wei. It may not be used by the latest VM, but it is kept for the backward compatibility.
    // pub l1_gas_price: u64,
    // /// The minimal price for the pubdata in Wei that the operator agrees on. Starting from the new fee model VM,
    // /// this value must also account for the fact that batch might be sealed because of the running out of compute.
    // pub fair_l2_gas_price: u64,
    // /// The minimal price for the pubdata in Wei that the operator agrees on.
    // /// This value must also account for the fact that batch might be sealed because of the running out of compute.
    // pub fair_pubdata_price: u64,
    pub fee_account: Address,
    pub enforced_base_fee: Option<u64>,
    pub first_l2_block: L2BlockEnv,
}

impl L1BatchEnv {
    pub fn base_fee(&self, vm_version: VmVersion) -> u64 {
        if let Some(base_fee) = self.enforced_base_fee {
            return base_fee;
        }

        let (base_fee, _) = derive_base_fee_and_gas_per_pubdata(self.fee_input, vm_version);
        base_fee
    }
    pub(crate) fn batch_gas_price_per_pubdata(&self, vm_version: VmVersion) -> u64 {
        derive_base_fee_and_gas_per_pubdata(self.fee_input, vm_version).1
    }
}

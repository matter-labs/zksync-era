use std::collections::HashMap;

use crate::vm_latest::L2BlockEnv;
use zk_evm_1_3_3::address_to_u256;
use zksync_types::{Address, L1BatchNumber, H256, U256};
use zksync_utils::h256_to_u256;

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
}

impl L1BatchEnv {
    const OPERATOR_ADDRESS_SLOT: usize = 0;
    const PREV_BLOCK_HASH_SLOT: usize = 1;
    const NEW_BLOCK_TIMESTAMP_SLOT: usize = 2;
    const NEW_BLOCK_NUMBER_SLOT: usize = 3;
    const L1_GAS_PRICE_SLOT: usize = 4;
    const FAIR_L2_GAS_PRICE_SLOT: usize = 5;
    const EXPECTED_BASE_FEE_SLOT: usize = 6;
    const SHOULD_SET_NEW_BLOCK_SLOT: usize = 7;

    /// Returns the initial memory for the bootloader based on the current batch environment.
    pub(crate) fn bootloader_initial_memory(&self) -> Vec<(usize, U256)> {
        let mut base_params: HashMap<usize, U256> = vec![
            (
                Self::OPERATOR_ADDRESS_SLOT,
                address_to_u256(&self.fee_account),
            ),
            (Self::PREV_BLOCK_HASH_SLOT, Default::default()),
            (Self::NEW_BLOCK_TIMESTAMP_SLOT, U256::from(self.timestamp)),
            (Self::NEW_BLOCK_NUMBER_SLOT, U256::from(self.number.0)),
            (Self::L1_GAS_PRICE_SLOT, U256::from(self.l1_gas_price)),
            (
                Self::FAIR_L2_GAS_PRICE_SLOT,
                U256::from(self.fair_l2_gas_price),
            ),
            (Self::EXPECTED_BASE_FEE_SLOT, U256::from(self.base_fee())),
            (Self::SHOULD_SET_NEW_BLOCK_SLOT, U256::from(0u32)),
        ]
        .into_iter()
        .collect();

        if let Some(prev_block_hash) = self.previous_batch_hash {
            base_params.insert(Self::PREV_BLOCK_HASH_SLOT, h256_to_u256(prev_block_hash));
            base_params.insert(Self::SHOULD_SET_NEW_BLOCK_SLOT, U256::from(1u32));
        }
        base_params.into_iter().collect()
    }

    pub(crate) fn block_gas_price_per_pubdata(&self) -> u64 {
        derive_base_fee_and_gas_per_pubdata(self.l1_gas_price, self.fair_l2_gas_price).1
    }
}

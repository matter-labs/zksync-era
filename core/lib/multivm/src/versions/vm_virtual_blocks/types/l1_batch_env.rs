use crate::interface::L1BatchEnv;
use std::collections::HashMap;
use zksync_types::U256;
use zksync_utils::{address_to_u256, h256_to_u256};

const OPERATOR_ADDRESS_SLOT: usize = 0;
const PREV_BLOCK_HASH_SLOT: usize = 1;
const NEW_BLOCK_TIMESTAMP_SLOT: usize = 2;
const NEW_BLOCK_NUMBER_SLOT: usize = 3;
const L1_GAS_PRICE_SLOT: usize = 4;
const FAIR_L2_GAS_PRICE_SLOT: usize = 5;
const EXPECTED_BASE_FEE_SLOT: usize = 6;
const SHOULD_SET_NEW_BLOCK_SLOT: usize = 7;

/// Returns the initial memory for the bootloader based on the current batch environment.
pub(crate) fn bootloader_initial_memory(l1_batch_env: &L1BatchEnv) -> Vec<(usize, U256)> {
    let mut base_params: HashMap<usize, U256> = vec![
        (
            OPERATOR_ADDRESS_SLOT,
            address_to_u256(&l1_batch_env.fee_account),
        ),
        (PREV_BLOCK_HASH_SLOT, Default::default()),
        (NEW_BLOCK_TIMESTAMP_SLOT, U256::from(l1_batch_env.timestamp)),
        (NEW_BLOCK_NUMBER_SLOT, U256::from(l1_batch_env.number.0)),
        (L1_GAS_PRICE_SLOT, U256::from(l1_batch_env.l1_gas_price)),
        (
            FAIR_L2_GAS_PRICE_SLOT,
            U256::from(l1_batch_env.fair_l2_gas_price),
        ),
        (EXPECTED_BASE_FEE_SLOT, U256::from(l1_batch_env.base_fee())),
        (SHOULD_SET_NEW_BLOCK_SLOT, U256::from(0u32)),
    ]
    .into_iter()
    .collect();

    if let Some(prev_block_hash) = l1_batch_env.previous_batch_hash {
        base_params.insert(PREV_BLOCK_HASH_SLOT, h256_to_u256(prev_block_hash));
        base_params.insert(SHOULD_SET_NEW_BLOCK_SLOT, U256::from(1u32));
    }
    base_params.into_iter().collect()
}

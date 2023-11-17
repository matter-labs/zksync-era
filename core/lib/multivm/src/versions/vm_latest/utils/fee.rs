//! Utility functions for vm
use zksync_system_constants::{
    MAX_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT, MAX_PUBDATA_PER_L1_BATCH,
};
use zksync_utils::ceil_div;

use crate::vm_latest::constants::BLOCK_OVERHEAD_L1_GAS;
use zksync_types::U256;

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub fn derive_base_fee_and_gas_per_pubdata(
    pubdata_price: u64,
    operator_gas_price: u64,
) -> (u64, u64) {
    // The baseFee is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        operator_gas_price,
        ceil_div(pubdata_price, MAX_GAS_PER_PUBDATA_BYTE),
    );

    let gas_per_pubdata = ceil_div(pubdata_price, base_fee);

    (base_fee, gas_per_pubdata)
}

/// Returns the operator's gas price based on the L1 gas price and the minimal gas price.
pub fn get_operator_gas_price(l1_gas_price: u64, minimal_gas_price: u64) -> u64 {
    let block_overhead_eth = U256::from(l1_gas_price) * U256::from(BLOCK_OVERHEAD_L1_GAS);

    // todo: maybe either use a different constant or use coeficients struct
    let block_overhead_part = block_overhead_eth / U256::from(MAX_L2_TX_GAS_LIMIT * 10);

    minimal_gas_price + block_overhead_part.as_u64()
}

pub fn get_operator_pubdata_price(l1_gas_price: u64, l1_pubdata_price: u64) -> u64 {
    let block_overhead_eth = U256::from(l1_gas_price) * U256::from(BLOCK_OVERHEAD_L1_GAS);

    // todo: maybe either use a different constant or use coeficients struct
    let block_overhead_part = block_overhead_eth / U256::from(MAX_PUBDATA_PER_L1_BATCH);

    l1_pubdata_price + block_overhead_part.as_u64()
}

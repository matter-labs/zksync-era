//! Utility functions for vm
use zksync_system_constants::{
    MAX_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT, MAX_PUBDATA_PER_L1_BATCH,
};
use zksync_types::U256;
use zksync_utils::ceil_div;

use crate::vm_latest::{
    constants::BLOCK_OVERHEAD_L1_GAS, old_vm::utils::eth_price_per_pubdata_byte,
};

/// Calculates the amount of gas required to publish one byte of pubdata
pub fn base_fee_to_gas_per_pubdata(l1_gas_price: u64, base_fee: u64) -> u64 {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);

    ceil_div(eth_price_per_pubdata_byte, base_fee)
}

/// TODO: possibly fix this method to explicitly use pubdata price
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

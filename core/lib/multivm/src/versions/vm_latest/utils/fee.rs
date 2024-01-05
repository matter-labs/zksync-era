//! Utility functions for vm
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_types::U256;
use zksync_utils::ceil_div;

use crate::vm_latest::old_vm::utils::eth_price_per_pubdata_byte;

/// Calculates the amount of gas required to publish one byte of pubdata
pub fn base_fee_to_gas_per_pubdata(l1_gas_price: u64, base_fee: u64) -> u64 {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);

    ceil_div(eth_price_per_pubdata_byte, base_fee)
}

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub(crate) fn derive_base_fee_and_gas_per_pubdata(
    l1_gas_price: u64,
    fair_gas_price: u64,
) -> (u64, u64) {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);

    // The `baseFee` is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fair_gas_price,
        ceil_div(eth_price_per_pubdata_byte, MAX_GAS_PER_PUBDATA_BYTE),
    );

    (
        base_fee,
        base_fee_to_gas_per_pubdata(l1_gas_price, base_fee),
    )
}

/// Changes the fee model output so that the expected gas per pubdata is smaller than or the `tx_gas_per_pubdata_limit`.
/// This function expects that the currently expected gas per pubdata is greater than the `tx_gas_per_pubdata_limit`.
pub(crate) fn adjust_l1_gas_price_for_tx(
    fair_l2_gas_price: u64,
    tx_gas_per_pubdata_limit: U256,
) -> u64 {
    // gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)
    // gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1
    // fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice
    let l1_gas_price = U256::from(fair_l2_gas_price)
        * (tx_gas_per_pubdata_limit - U256::from(1u32))
        / U256::from(17);

    l1_gas_price.as_u64()
}

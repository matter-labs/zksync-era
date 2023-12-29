//! Utility functions for vm
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{fee_model::FeeModelOutput, U256};
use zksync_utils::ceil_div;

use crate::vm_latest::old_vm::utils::eth_price_per_pubdata_byte;

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

pub fn adjust_pubdata_price_for_tx(fee_model: &mut FeeModelOutput, tx_gas_per_pubdata_limit: U256) {
    let (_, current_pubdata_price) = derive_base_fee_and_gas_per_pubdata(
        fee_model.fair_pubdata_price,
        fee_model.fair_l2_gas_price,
    );

    let new_fair_pubdata_price = if U256::from(current_pubdata_price) > tx_gas_per_pubdata_limit {
        // gasPerPubdata = ceil(pubdata_price / fair_l2_gas_price)
        // gasPerPubdata <= pubdata_price / fair_l2_gas_price + 1
        // fair_l2_gas_price(gasPerPubdata - 1) <= pubdata_price
        U256::from(fee_model.fair_l2_gas_price) * (tx_gas_per_pubdata_limit - U256::from(1u32))
    } else {
        return;
    };

    fee_model.fair_pubdata_price = new_fair_pubdata_price.as_u64();
}

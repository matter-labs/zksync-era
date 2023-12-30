//! Utility functions for vm
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{fee_model::BatchFeeModelInput, U256};
use zksync_utils::ceil_div;

use crate::vm_latest::old_vm::utils::eth_price_per_pubdata_byte;

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub fn derive_base_fee_and_gas_per_pubdata(
    fair_pubdata_price: u64,
    fair_l2_gas_price: u64,
) -> (u64, u64) {
    // The baseFee is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fair_l2_gas_price,
        ceil_div(fair_pubdata_price, MAX_GAS_PER_PUBDATA_BYTE),
    );

    let gas_per_pubdata = ceil_div(fair_pubdata_price, base_fee);

    (base_fee, gas_per_pubdata)
}

/// Changes the fee model output so that the expected gas per pubdata is smaller than or the `tx_gas_per_pubdata_limit`.
pub fn adjust_pubdata_price_for_tx(
    batch_fee_input: &mut BatchFeeModelInput,
    tx_gas_per_pubdata_limit: U256,
) {
    let (_, current_pubdata_price) = derive_base_fee_and_gas_per_pubdata(
        batch_fee_input.fair_pubdata_price,
        batch_fee_input.fair_l2_gas_price,
    );

    let new_fair_pubdata_price = if U256::from(current_pubdata_price) > tx_gas_per_pubdata_limit {
        // gasPerPubdata = ceil(fair_pubdata_price / fair_l2_gas_price)
        // gasPerPubdata <= fair_pubdata_price / fair_l2_gas_price + 1
        // fair_l2_gas_price(gasPerPubdata - 1) <= fair_pubdata_price
        U256::from(batch_fee_input.fair_l2_gas_price)
            * (tx_gas_per_pubdata_limit - U256::from(1u32))
    } else {
        return;
    };

    batch_fee_input.fair_pubdata_price = new_fair_pubdata_price.as_u64();
}

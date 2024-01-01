//! Utility functions for vm
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE_PRE_1_4_1;
use zksync_types::fee_model::L1PeggedBatchFeeModelInput;
use zksync_utils::ceil_div;

use crate::vm_virtual_blocks::old_vm::utils::eth_price_per_pubdata_byte;

/// Calculates the amount of gas required to publish one byte of pubdata
pub(crate) fn base_fee_to_gas_per_pubdata(l1_gas_price: u64, base_fee: u64) -> u64 {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);

    ceil_div(eth_price_per_pubdata_byte, base_fee)
}

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub(crate) fn derive_base_fee_and_gas_per_pubdata(
    fee_input: L1PeggedBatchFeeModelInput,
) -> (u64, u64) {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(fee_input.l1_gas_price);

    // The baseFee is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fee_input.fair_l2_gas_price,
        ceil_div(
            eth_price_per_pubdata_byte,
            MAX_GAS_PER_PUBDATA_BYTE_PRE_1_4_1,
        ),
    );

    (
        base_fee,
        base_fee_to_gas_per_pubdata(fee_input.l1_gas_price, base_fee),
    )
}

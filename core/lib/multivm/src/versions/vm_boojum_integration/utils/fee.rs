//! Utility functions for vm
use zksync_types::{fee_model::L1PeggedBatchFeeModelInput, U256};
use zksync_utils::ceil_div_u256;

use crate::{
    vm_boojum_integration::{
        constants::MAX_GAS_PER_PUBDATA_BYTE, old_vm::utils::eth_price_per_pubdata_byte,
    },
    vm_latest::L1BatchEnv,
};

/// Calculates the amount of gas required to publish one byte of pubdata
pub fn base_fee_to_gas_per_pubdata(l1_gas_price: U256, base_fee: U256) -> U256 {
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);
    ceil_div_u256(eth_price_per_pubdata_byte, base_fee)
}

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub(crate) fn derive_base_fee_and_gas_per_pubdata(
    fee_input: L1PeggedBatchFeeModelInput,
) -> (U256, U256) {
    let L1PeggedBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price,
    } = fee_input;
    let eth_price_per_pubdata_byte = eth_price_per_pubdata_byte(l1_gas_price);

    // The `baseFee` is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fair_l2_gas_price,
        ceil_div_u256(
            eth_price_per_pubdata_byte,
            U256::from(MAX_GAS_PER_PUBDATA_BYTE),
        ),
    );

    (
        base_fee,
        base_fee_to_gas_per_pubdata(l1_gas_price, base_fee),
    )
}

pub(crate) fn get_batch_base_fee(l1_batch_env: &L1BatchEnv) -> U256 {
    if let Some(base_fee) = l1_batch_env.enforced_base_fee {
        return base_fee;
    }
    let (base_fee, _) =
        derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input.into_l1_pegged());
    base_fee
}

pub(crate) fn get_batch_gas_per_pubdata(l1_batch_env: &L1BatchEnv) -> U256 {
    derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input.into_l1_pegged()).1
}

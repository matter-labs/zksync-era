//! Utility functions for vm
use zksync_types::{fee_model::PubdataIndependentBatchFeeModelInput, U256};
use zksync_utils::ceil_div_u256;

use crate::vm_latest::{constants::MAX_GAS_PER_PUBDATA_BYTE, L1BatchEnv};

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub(crate) fn derive_base_fee_and_gas_per_pubdata(
    fee_input: PubdataIndependentBatchFeeModelInput,
) -> (U256, U256) {
    let PubdataIndependentBatchFeeModelInput {
        fair_l2_gas_price,
        fair_pubdata_price,
        ..
    } = fee_input;

    // The `baseFee` is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fair_l2_gas_price,
        ceil_div_u256(fair_pubdata_price, U256::from(MAX_GAS_PER_PUBDATA_BYTE)),
    );

    let gas_per_pubdata = ceil_div_u256(fair_pubdata_price, base_fee);

    (base_fee, gas_per_pubdata)
}

pub(crate) fn get_batch_base_fee(l1_batch_env: &L1BatchEnv) -> U256 {
    if let Some(base_fee) = l1_batch_env.enforced_base_fee {
        return base_fee;
    }
    let (base_fee, _) =
        derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input.into_pubdata_independent());
    base_fee
}

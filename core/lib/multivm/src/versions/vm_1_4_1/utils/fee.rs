//! Utility functions for vm
use zksync_types::fee_model::PubdataIndependentBatchFeeModelInput;
use zksync_utils::ceil_div;

use crate::{interface::L1BatchEnv, vm_1_4_1::constants::MAX_GAS_PER_PUBDATA_BYTE};

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub(crate) fn derive_base_fee_and_gas_per_pubdata(
    fee_input: PubdataIndependentBatchFeeModelInput,
) -> (u64, u64) {
    let PubdataIndependentBatchFeeModelInput {
        fair_l2_gas_price,
        fair_pubdata_price,
        ..
    } = fee_input;

    // The `baseFee` is set in such a way that it is always possible for a transaction to
    // publish enough public data while compensating us for it.
    let base_fee = std::cmp::max(
        fair_l2_gas_price,
        ceil_div(fair_pubdata_price, MAX_GAS_PER_PUBDATA_BYTE),
    );

    let gas_per_pubdata = ceil_div(fair_pubdata_price, base_fee);

    (base_fee, gas_per_pubdata)
}

pub(crate) fn get_batch_base_fee(l1_batch_env: &L1BatchEnv) -> u64 {
    if let Some(base_fee) = l1_batch_env.enforced_base_fee {
        return base_fee;
    }
    let (base_fee, _) =
        derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input.into_pubdata_independent());
    base_fee
}

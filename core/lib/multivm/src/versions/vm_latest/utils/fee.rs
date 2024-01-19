//! Utility functions for vm
use zksync_types::{
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    U256,
};
use zksync_utils::ceil_div;

use crate::vm_latest::{constants::MAX_GAS_PER_PUBDATA_BYTE, L1BatchEnv};

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

/// Changes the fee model output so that the expected gas per pubdata is smaller than or the `tx_gas_per_pubdata_limit`.
/// This function expects that the currently expected gas per pubdata is greater than the `tx_gas_per_pubdata_limit`.
pub(crate) fn adjust_pubdata_price_for_tx(
    mut batch_fee_input: BatchFeeInput,
    tx_gas_per_pubdata_limit: U256,
) -> BatchFeeInput {
    match &mut batch_fee_input {
        BatchFeeInput::L1Pegged(fee_input) => {
            // `gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)`
            // `gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1`
            // `fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice`
            let new_l1_gas_price = U256::from(fee_input.fair_l2_gas_price)
                * (tx_gas_per_pubdata_limit - U256::from(1u32))
                / U256::from(17);

            fee_input.l1_gas_price = new_l1_gas_price.as_u64();
        }
        BatchFeeInput::PubdataIndependent(fee_input) => {
            // `gasPerPubdata = ceil(fair_pubdata_price / fair_l2_gas_price)`
            // `gasPerPubdata <= fair_pubdata_price / fair_l2_gas_price + 1`
            // `fair_l2_gas_price(gasPerPubdata - 1) <= fair_pubdata_price`
            let new_fair_pubdata_price = U256::from(fee_input.fair_l2_gas_price)
                * (tx_gas_per_pubdata_limit - U256::from(1u32));

            fee_input.fair_pubdata_price = new_fair_pubdata_price.as_u64();
        }
    }

    batch_fee_input
}

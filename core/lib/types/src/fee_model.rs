use serde::{Deserialize, Serialize};
use serde_with::DeserializeFromStr;
use zksync_basic_types::U256;
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_utils::ceil_div;

/// Output to be provided into the VM
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeModelOutput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub fair_pubdata_price: u64,

    /// The L1 gas price to provide to the VM
    pub l1_gas_price: u64,
}

impl FeeModelOutput {
    pub fn adjust_pubdata_price_for_tx(&mut self, tx_gas_per_pubdata_limit: U256) {
        let (_, current_pubdata_price) =
            derive_base_fee_and_gas_per_pubdata(self.fair_pubdata_price, self.fair_l2_gas_price);

        let new_fair_pubdata_price = if U256::from(current_pubdata_price) > tx_gas_per_pubdata_limit
        {
            // gasPerPubdata = ceil(pubdata_price / fair_l2_gas_price)
            // gasPerPubdata <= pubdata_price / fair_l2_gas_price + 1
            // fair_l2_gas_price(gasPerPubdata - 1) <= pubdata_price
            U256::from(self.fair_l2_gas_price) * (tx_gas_per_pubdata_limit - U256::from(1u32))
        } else {
            return;
        };

        self.fair_pubdata_price = new_fair_pubdata_price.as_u64();
    }
}

/// TODO: possibly remove this method from here
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

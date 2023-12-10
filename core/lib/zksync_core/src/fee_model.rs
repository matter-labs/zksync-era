use multivm::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;
use zksync_system_constants::{GAS_PER_PUBDATA_BYTE, L1_GAS_PER_PUBDATA_BYTE};
use zksync_types::U256;

/// Structure that represents the logic of the fee model.
/// It is responsible for setting the corresponding L1 and L2 gas prices.
pub(crate) struct FeeModel {
    // Base L1 gas price,
    base_l1_gas_price: u64,
    base_l2_gas_price: u64,

    compute_overhead_percent: f64,
    pubdata_overhead_percent: f64,
    batch_overhead_l1_gas: u64,
    max_gas_per_batch: u64,
    max_pubdata_per_batch: u64,

    fair_l2_gas_price: u64,
    // This variable is used to derived fair_pubdata_price.
    // This struct must maintain the invariant that fair_l1_gas_price * 17 = fair_pubdata_price
    // and so we maintain only fair_l1_gas_price so these two values are always in sync.
    fair_l1_gas_price: u64,
}

/// Output to be provided into the VM
pub(crate) struct FeeModelOutput {
    /// Fair L2 gas price to provide
    pub(crate) fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub(crate) fair_pubdata_price: u64,

    /// The L1 gas price to provide to the VM
    pub(crate) l1_gas_price: u64,
}

impl FeeModel {
    pub(crate) fn new(
        base_l1_gas_price: u64,
        base_l2_gas_price: u64,
        compute_overhead_percent: f64,
        pubdata_overhead_percent: f64,
        batch_overhead_l1_gas: u64,
        max_gas_per_batch: u64,
        max_pubdata_per_batch: u64,
    ) -> Self {
        let fair_l2_gas_price = {
            let compute_overhead_wei =
                (batch_overhead_l1_gas as f64 * compute_overhead_percent) as u64;
            let gas_overhead_wei = compute_overhead_wei / max_gas_per_batch;

            base_l2_gas_price + gas_overhead_wei
        };

        let fair_pubdata_price = {
            let base_pubdata_price_wei = base_l1_gas_price * (L1_GAS_PER_PUBDATA_BYTE as u64);

            let pubdata_overhead_wei =
                (batch_overhead_l1_gas as f64 * pubdata_overhead_percent) as u64;
            let pubdata_overhead_wei = pubdata_overhead_wei / max_pubdata_per_batch;

            let pubdata_price_wei = base_pubdata_price_wei + pubdata_overhead_wei;
            let remainder = pubdata_price_wei % (L1_GAS_PER_PUBDATA_BYTE as u64);

            // In this version, the pubdata price will be strictly derived from L1 gas price in the
            // bootloader and will be equal to L1_GAS_PER_PUBDATA_BYTE * L1 gas price.
            // Also, the bootloader forbids using 0 as L1 gas price.
            if remainder != 0 || pubdata_price_wei == 0 {
                pubdata_price_wei + (L1_GAS_PER_PUBDATA_BYTE as u64 - remainder)
            } else {
                pubdata_price_wei
            }
        };

        // Just in case
        assert!(
            fair_pubdata_price % (L1_GAS_PER_PUBDATA_BYTE as u64) == 0,
            "The pubdata price must be divisible by L1_GAS_PER_PUBDATA_BYTE"
        );

        let l1_gas_price = fair_pubdata_price / (L1_GAS_PER_PUBDATA_BYTE as u64);

        assert!(l1_gas_price > 0, "L1 gas price must be non-zero");
        assert!(
            l1_gas_price >= base_l1_gas_price,
            "L1 gas price must be greater than base L1 gas price"
        );

        Self {
            base_l1_gas_price,
            base_l2_gas_price,
            compute_overhead_percent,
            pubdata_overhead_percent,
            batch_overhead_l1_gas,
            max_gas_per_batch,
            max_pubdata_per_batch,

            fair_l2_gas_price,
            fair_l1_gas_price: l1_gas_price,
        }
    }

    pub(crate) fn adjust_pubdata_price_for_tx(&mut self, tx_gas_per_pubdata_limit: U256) {
        let (_, current_pubdata_price) =
            derive_base_fee_and_gas_per_pubdata(self.fair_l1_gas_price, self.fair_l2_gas_price);

        let new_fair_l1_gas_price = if U256::from(current_pubdata_price) > tx_gas_per_pubdata_limit
        {
            // gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)
            // gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1
            // fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice
            let l1_gas_price = U256::from(self.fair_l2_gas_price)
                * (tx_gas_per_pubdata_limit - U256::from(1u32))
                / U256::from(17);

            l1_gas_price.as_u64()
        } else {
            return;
        };

        self.fair_l1_gas_price = new_fair_l1_gas_price;
    }

    /// The logic of the fee model is the following one:
    /// - The price for ergs is defined as `base_l2_gas_price + compute_overhead_wei / max_gas_per_batch`, where
    /// compute_overhead_wei should represent the possibility that the batch will be closed because of the overuse of the
    /// computation on L2.
    /// - The price for pubdata is defined as `base_l1_gas_price + pubdata_overhead_wei / max_pubdata_per_batch`, where
    /// pubdata_overhead_wei should represent the possibility that the batch will be closed because of the overuse of the
    /// pubdata.
    ///
    /// The outputs of this struct are to be used together with the version of the bootloader that still uses the L1 gas price
    /// to derive the pubdata price. And thus, the pubdata price will be equal to `L1_GAS_PER_PUBDATA_BYTE * L1 gas price`.
    pub(crate) fn get_output(&self) -> FeeModelOutput {
        FeeModelOutput {
            fair_l2_gas_price: self.fair_l2_gas_price,
            fair_pubdata_price: self.fair_l1_gas_price * L1_GAS_PER_PUBDATA_BYTE as u64,
            l1_gas_price: self.fair_l1_gas_price,
        }
    }
}

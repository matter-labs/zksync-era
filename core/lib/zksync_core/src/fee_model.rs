use multivm::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;
use zksync_types::U256;

/// Structure that represents the logic of the fee model.
/// It is responsible for setting the corresponding L1 and L2 gas prices.
#[derive(Debug, Copy, Clone)]
pub(crate) struct FeeModel {
    /// The assumed L1 gas price.
    base_l1_gas_price: u64,
    /// The assumed L1 pubdata price.
    base_pubdata_price: u64,
    /// The assumed L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    base_l2_gas_price: u64,

    /// The proposed fair L2 gas price based on the current base L2 gas price and the overhead (i.e. the possibility
    /// that the batch will be closed because of the overuse of circuits as well as bootloader memory).
    fair_l2_gas_price: u64,

    /// The proposed fair pubdata price on the current base pubdata price and the overhead (i.e. the possibility
    /// that the batch will be closed because of the overuse of pubdata).
    fair_pubdata_price: u64,
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
        base_pubdata_price: u64,
        base_l2_gas_price: u64,
        compute_overhead_percent: f64,
        pubdata_overhead_percent: f64,
        batch_overhead_l1_gas: u64,
        max_gas_per_batch: u64,
        max_pubdata_per_batch: u64,
    ) -> Self {
        // FIXME: overflow is possible
        let l1_batch_overhead_wei = base_l1_gas_price * batch_overhead_l1_gas;

        let fair_l2_gas_price = {
            let compute_overhead_wei =
                (l1_batch_overhead_wei as f64 * compute_overhead_percent) as u64;
            let gas_overhead_wei = compute_overhead_wei / max_gas_per_batch;

            base_l2_gas_price + gas_overhead_wei
        };

        let fair_pubdata_price = {
            let pubdata_overhead_wei =
                (l1_batch_overhead_wei as f64 * pubdata_overhead_percent) as u64;
            let pubdata_overhead_wei = pubdata_overhead_wei / max_pubdata_per_batch;

            base_pubdata_price + pubdata_overhead_wei
        };

        Self {
            base_l1_gas_price,
            base_pubdata_price,
            base_l2_gas_price,

            fair_l2_gas_price,
            fair_pubdata_price,
        }
    }

    pub(crate) fn adjust_pubdata_price_for_tx(&mut self, tx_gas_per_pubdata_limit: U256) {
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

    /// The logic of the fee model is the following one:
    /// - The price for ergs is defined as `base_l2_gas_price + compute_overhead_wei / max_gas_per_batch`, where
    /// compute_overhead_wei should represent the possibility that the batch will be closed because of the overuse of the
    /// computation on L2.
    /// - The price for pubdata is defined as `base_l1_gas_price + pubdata_overhead_wei / max_pubdata_per_batch`, where
    /// pubdata_overhead_wei should represent the possibility that the batch will be closed because of the overuse of the
    /// pubdata.
    pub(crate) fn get_output(&self) -> FeeModelOutput {
        FeeModelOutput {
            fair_l2_gas_price: self.fair_l2_gas_price,
            fair_pubdata_price: self.fair_pubdata_price,
            l1_gas_price: self.base_l1_gas_price,
        }
    }
}

use zksync_system_constants::{GAS_PER_PUBDATA_BYTE, L1_GAS_PER_PUBDATA_BYTE};

struct FeeModel {
    // Base L1 gas price,
    base_l1_gas_price: u64,
    base_l2_gas_price: u64,

    compute_overhead_percent: f64,
    pubdata_overhead_percent: f64,
    batch_overhead_l1_gas: u64,
    max_gas_per_batch: u64,
    max_pubdata_per_batch: u64,
}

pub(crate) struct FeeModelOutput {
    /// Fair L2 gas price to provide
    pub(crate) fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub(crate) fair_pubdata_price: u64,
    pub(crate) l1_gas_price: u64,
}

impl FeeModel {
    fn new(
        base_l1_gas_price: u64,
        base_l2_gas_price: u64,
        compute_overhead_percent: f64,
        pubdata_overhead_percent: f64,
        batch_overhead_l1_gas: u64,
        max_gas_per_batch: u64,
    ) -> Self {
        Self {
            base_l1_gas_price,
            base_l2_gas_price,
            compute_overhead_percent,
            pubdata_overhead_percent,
            batch_overhead_l1_gas,
            max_gas_per_batch,
        }
    }

    fn get_fair_l2_gas_price(&self) -> u64 {
        // Fair L2 gas price is the price that the operator agrees to charge from the user.
    }

    pub fn get_output(&self) -> FeeModelOutput {
        let batch_overhead = self.batch_overhead_l1_gas * self.base_l1_gas_price;

        let fair_l2_gas_price = {
            let compute_overhead_wei =
                (batch_overhead as f64 * self.compute_overhead_percent) as u64;
            let gas_overhead_wei = compute_overhead_wei / self.max_gas_per_batch;

            self.base_l2_gas_price + gas_overhead_wei
        };

        let fair_pubdata_price = {
            let base_pubdata_price_wei = self.base_l1_gas_price * (L1_GAS_PER_PUBDATA_BYTE as u64);

            let pubdata_overhead_wei =
                (batch_overhead as f64 * self.pubdata_overhead_percent) as u64;
            let pubdata_overhead_wei = pubdata_overhead_wei / self.max_pubdata_per_batch;

            let pubdata_price_wei = base_pubdata_price_wei + pubdata_overhead_wei;
            let pubdata_price_wei = L1_GAS_PER_PUBDATA_BYTE;

            if pubdata_price_wei % L1_GAS_PER_PUBDATA_BYTE != 0 {
                pubdata_price_wei
                    + (L1_GAS_PER_PUBDATA_BYTE - (pubdata_price_wei % L1_GAS_PER_PUBDATA_BYTE))
            } else {
                pubdata_price_wei
            }
        };

        FeeModelOutput {
            fair_l2_gas_price: fair_l2_gas_price,
            fair_pubdata_price: 0,
            l1_gas_price: 0,
        }
    }
}

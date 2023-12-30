use std::sync::Arc;

use zksync_types::{fee_model::BatchFeeModelInput, U256};

use crate::l1_gas_price::L1GasPriceProvider;

/// Trait responsiblef for providign fee info for a batch
pub trait BatchFeeModelInputProvider {
    fn get_fee_model_params(&self, scale_l1_prices: bool) -> BatchFeeModelInput;
}

pub(crate) struct MainNodeFeeInputProvider<G> {
    provider: Arc<G>,
    config: MainNodeFeeModelConfig,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MainNodeFeeModelConfig {
    pub(crate) l1_gas_price_scale_factor: f64,
    pub(crate) l1_pubdata_price_scale_factor: f64,
    pub(crate) minimal_l2_gas_price: u64,
    pub(crate) compute_overhead_percent: f64,
    pub(crate) pubdata_overhead_percent: f64,
    pub(crate) batch_overhead_l1_gas: u64,
    pub(crate) max_gas_per_batch: u64,
    pub(crate) max_pubdata_per_batch: u64,
}

impl<G: L1GasPriceProvider> BatchFeeModelInputProvider for MainNodeFeeInputProvider<G> {
    fn get_fee_model_params(&self, scale_l1_prices: bool) -> BatchFeeModelInput {
        let l1_gas_price = self.provider.estimate_effective_gas_price();
        let l1_pubdata_price = self.provider.estimate_effective_pubdata_price();

        let MainNodeFeeModelConfig {
            l1_gas_price_scale_factor,
            l1_pubdata_price_scale_factor,
            minimal_l2_gas_price,
            compute_overhead_percent,
            pubdata_overhead_percent,
            batch_overhead_l1_gas,
            max_gas_per_batch,
            max_pubdata_per_batch,
        } = self.config;

        let (l1_gas_price_scale_factor, l1_pubdata_price_scale_factor) = if scale_l1_prices {
            (l1_gas_price_scale_factor, l1_pubdata_price_scale_factor)
        } else {
            (1.0, 1.0)
        };

        let base_l1_gas_price = (l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;
        let base_pubdata_price = (l1_pubdata_price as f64 * l1_pubdata_price_scale_factor) as u64;

        let fee_model = compute_batch_fee_model_input(
            base_l1_gas_price,
            base_pubdata_price,
            minimal_l2_gas_price,
            compute_overhead_percent,
            pubdata_overhead_percent,
            batch_overhead_l1_gas,
            max_gas_per_batch,
            max_pubdata_per_batch,
        );

        fee_model
    }
}

impl<G: L1GasPriceProvider> MainNodeFeeInputProvider<G> {
    pub(crate) fn new(provider: Arc<G>, config: MainNodeFeeModelConfig) -> Self {
        Self { provider, config }
    }
}

/// Accepts the input for the fee model and returns the parameters to provide into the VM.
/// - `base_l1_gas_price` - the assumed L1 gas price.
/// - `base_pubdata_price` - the assumed L1 pubdata price.
/// - `base_l2_gas_price` - the assumed L2 gas price, i.e. the price that should include the cost of computation/proving as well
/// as potentially premium for congestion.
/// - `compute_overhead_percent` - the constant that represents the possibility that a batch can be sealed because of overuse of compute.
/// - `pubdata_overhead_percent` - the constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
/// - `batch_overhead_l1_gas` - the constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
/// - `max_gas_per_batch` - the maximum amount of gas that can be used by the batch.
/// - `max_pubdata_per_batch` - the maximum amount of pubdata that can be used by the batch.
fn compute_batch_fee_model_input(
    base_l1_gas_price: u64,
    base_pubdata_price: u64,
    base_l2_gas_price: u64,
    compute_overhead_percent: f64,
    pubdata_overhead_percent: f64,
    batch_overhead_l1_gas: u64,
    max_gas_per_batch: u64,
    max_pubdata_per_batch: u64,
) -> BatchFeeModelInput {
    // While the final results of the calculations are not expected to have any overflows, the intermediate computations
    // might, so we use U256 for them.
    let l1_batch_overhead_wei = U256::from(base_l1_gas_price) * U256::from(batch_overhead_l1_gas);

    let fair_l2_gas_price = {
        let l1_batch_overhead_per_gas = l1_batch_overhead_wei / U256::from(max_gas_per_batch);
        let gas_overhead_wei =
            (l1_batch_overhead_per_gas.as_u64() as f64 * compute_overhead_percent) as u64;

        base_l2_gas_price + gas_overhead_wei
    };

    let fair_pubdata_price = {
        let l1_batch_overhead_per_pubdata =
            l1_batch_overhead_wei / U256::from(max_pubdata_per_batch);
        let pubdata_overhead_wei =
            (l1_batch_overhead_per_pubdata.as_u64() as f64 * pubdata_overhead_percent) as u64;
        let pubdata_overhead_wei = pubdata_overhead_wei / max_pubdata_per_batch;

        pubdata_overhead_wei
    };

    BatchFeeModelInput {
        l1_gas_price: base_l1_gas_price,
        fair_l2_gas_price,
        fair_pubdata_price,
    }
}

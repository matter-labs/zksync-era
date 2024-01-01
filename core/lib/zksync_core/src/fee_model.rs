use std::sync::Arc;

use zksync_types::{
    fee_model::{
        BatchFeeModelInput, L1PeggedBatchFeeModelInput, MainNodeFeeModelConfig, MainNodeFeeParams,
        PubdataIndependentBatchFeeModelInput,
    },
    U256,
};

use crate::l1_gas_price::L1GasPriceProvider;

/// Trait responsiblef for providign fee info for a batch
pub trait BatchFeeModelInputProvider {
    fn get_batch_fee_input(&self, scale_l1_prices: bool) -> BatchFeeModelInput {
        // FIXME: use legacy by default or use a config to regulate it
        compute_legacy_batch_fee_model_input(self.get_fee_model_params(), scale_l1_prices)
    }

    fn get_fee_model_params(&self) -> MainNodeFeeParams;
}

pub(crate) struct MainNodeFeeInputProvider<G> {
    provider: Arc<G>,
    config: MainNodeFeeModelConfig,
}

impl<G: L1GasPriceProvider> BatchFeeModelInputProvider for MainNodeFeeInputProvider<G> {
    fn get_fee_model_params(&self) -> MainNodeFeeParams {
        let l1_gas_price = self.provider.estimate_effective_gas_price();
        let l1_pubdata_price = self.provider.estimate_effective_pubdata_price();

        MainNodeFeeParams {
            config: self.config,
            l1_gas_price,
            l1_pubdata_price,
        }
    }
}

impl<G: L1GasPriceProvider> MainNodeFeeInputProvider<G> {
    pub(crate) fn new(provider: Arc<G>, config: MainNodeFeeModelConfig) -> Self {
        Self { provider, config }
    }
}

pub(crate) fn compute_legacy_batch_fee_model_input(
    params: MainNodeFeeParams,
    scale_l1_prices: bool,
) -> BatchFeeModelInput {
    let l1_gas_price_scale_factor = if scale_l1_prices {
        params.config.l1_gas_price_scale_factor
    } else {
        1.0
    };

    let l1_gas_price = (params.l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;

    BatchFeeModelInput::L1Pegged(L1PeggedBatchFeeModelInput {
        l1_gas_price: l1_gas_price,
        fair_l2_gas_price: params.config.minimal_l2_gas_price,
    })
}

/// TOOD: this comment is incorrect.
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
pub(crate) fn compute_batch_fee_model_input(
    params: MainNodeFeeParams,
    scale_l1_prices: bool,
) -> BatchFeeModelInput {
    let MainNodeFeeParams {
        config,
        l1_gas_price,
        l1_pubdata_price,
    } = params;

    let MainNodeFeeModelConfig {
        l1_gas_price_scale_factor,
        l1_pubdata_price_scale_factor,
        minimal_l2_gas_price,
        compute_overhead_percent,
        pubdata_overhead_percent,
        batch_overhead_l1_gas,
        max_gas_per_batch,
        max_pubdata_per_batch,
    } = config;

    let (l1_gas_price_scale_factor, l1_pubdata_price_scale_factor) = if scale_l1_prices {
        (l1_gas_price_scale_factor, l1_pubdata_price_scale_factor)
    } else {
        (1.0, 1.0)
    };

    let l1_gas_price = (l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;
    let l1_pubdata_price = (l1_pubdata_price as f64 * l1_pubdata_price_scale_factor) as u64;

    // While the final results of the calculations are not expected to have any overflows, the intermediate computations
    // might, so we use U256 for them.
    let l1_batch_overhead_wei = U256::from(l1_gas_price) * U256::from(batch_overhead_l1_gas);

    let fair_l2_gas_price = {
        let l1_batch_overhead_per_gas = l1_batch_overhead_wei / U256::from(max_gas_per_batch);
        let gas_overhead_wei =
            (l1_batch_overhead_per_gas.as_u64() as f64 * compute_overhead_percent) as u64;

        minimal_l2_gas_price + gas_overhead_wei
    };

    let fair_pubdata_price = {
        let l1_batch_overhead_per_pubdata =
            l1_batch_overhead_wei / U256::from(max_pubdata_per_batch);
        let pubdata_overhead_wei =
            (l1_batch_overhead_per_pubdata.as_u64() as f64 * pubdata_overhead_percent) as u64;
        let pubdata_overhead_wei = pubdata_overhead_wei / max_pubdata_per_batch;

        l1_pubdata_price + pubdata_overhead_wei
    };

    BatchFeeModelInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price,
        fair_pubdata_price,
    })
}

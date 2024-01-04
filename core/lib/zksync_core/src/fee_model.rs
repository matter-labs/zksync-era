use std::{fmt, sync::Arc};

use zksync_types::{
    fee_model::{
        BatchFeeInput, L1PeggedBatchFeeModelInput, MainNodeFeeModelConfig, MainNodeFeeParams,
        PubdataIndependentBatchFeeModelInput,
    },
    U256,
};

use crate::l1_gas_price::L1GasPriceProvider;

/// Trait responsiblef for providign fee info for a batch
pub trait BatchFeeModelInputProvider: fmt::Debug + 'static + Send + Sync {
    fn get_batch_fee_input(&self, scale_l1_prices: bool) -> BatchFeeInput {
        // FIXME: use legacy by default or use a config to regulate it
        compute_legacy_batch_fee_model_input(self.get_fee_model_params(), scale_l1_prices)
    }

    fn get_fee_model_params(&self) -> MainNodeFeeParams;
}

#[derive(Debug)]
pub(crate) struct MainNodeFeeInputProvider<G: ?Sized> {
    provider: Arc<G>,
    config: MainNodeFeeModelConfig,
}

impl<G: L1GasPriceProvider + ?Sized> BatchFeeModelInputProvider for MainNodeFeeInputProvider<G> {
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

impl<G: L1GasPriceProvider + ?Sized> MainNodeFeeInputProvider<G> {
    pub(crate) fn new(provider: Arc<G>, config: MainNodeFeeModelConfig) -> Self {
        Self { provider, config }
    }
}

/// Calculates the batch fee input based on the main node parameters.
/// This function uses the legacy fee model, used prior to 1.4.1, i.e. where the pubdata price does not include the proving costs.
pub(crate) fn compute_legacy_batch_fee_model_input(
    params: MainNodeFeeParams,
    scale_l1_prices: bool,
) -> BatchFeeInput {
    let l1_gas_price_scale_factor = if scale_l1_prices {
        params.config.l1_gas_price_scale_factor
    } else {
        1.0
    };

    let l1_gas_price = (params.l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;

    BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
        l1_gas_price: l1_gas_price,
        fair_l2_gas_price: params.config.minimal_l2_gas_price,
    })
}

/// Calculates the batch fee input based on the main node parameters.
pub(crate) fn compute_batch_fee_model_input(
    params: MainNodeFeeParams,
    scale_l1_prices: bool,
) -> BatchFeeInput {
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
        ..
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

    BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price,
        fair_pubdata_price,
    })
}

use std::{fmt, sync::Arc};

use zksync_types::{
    fee_model::{
        BatchFeeInput, L1PeggedBatchFeeModelInput, MainNodeFeeModelConfig,
        MainNodeFeeModelConfigV2, MainNodeFeeParams, MainNodeFeeParamsV1, MainNodeFeeParamsV2,
        PubdataIndependentBatchFeeModelInput,
    },
    U256,
};

use crate::l1_gas_price::L1GasPriceProvider;

/// Trait responsible for providing fee info for a batch
pub trait BatchFeeModelInputProvider: fmt::Debug + 'static + Send + Sync {
    /// Returns the batch fee with scaling applied. This may be used to account for the fact that the L1 gas and pubdata prices may fluctuate, esp.
    /// in API methods that should return values that are valid for some period of time after the estimation was done.
    fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> BatchFeeInput {
        let params = self.get_fee_model_params();

        match params {
            MainNodeFeeParams::V1(params) => {
                compute_batch_fee_model_input_v1(params, l1_gas_price_scale_factor)
            }
            MainNodeFeeParams::V2(params) => compute_batch_fee_model_input_v2(
                params,
                l1_gas_price_scale_factor,
                l1_pubdata_price_scale_factor,
            ),
        }
    }

    /// Returns the batch fee input as-is, i.e. without any scaling for the L1 gas and pubdata prices.
    fn get_batch_fee_input(&self) -> BatchFeeInput {
        self.get_batch_fee_input_scaled(1.0, 1.0)
    }

    /// Returns the fee model parameters.
    fn get_fee_model_params(&self) -> MainNodeFeeParams;
}

/// The struct that represents the batch fee input provider to be used in the main node of the server, i.e.
/// it explicitly gets the L1 gas price from the provider and uses it to calculate the batch fee input instead of getting
/// it from other node.
#[derive(Debug)]
pub(crate) struct MainNodeFeeInputProvider<G: ?Sized> {
    provider: Arc<G>,
    config: MainNodeFeeModelConfig,
}

impl<G: L1GasPriceProvider + ?Sized> BatchFeeModelInputProvider for MainNodeFeeInputProvider<G> {
    fn get_fee_model_params(&self) -> MainNodeFeeParams {
        match self.config {
            MainNodeFeeModelConfig::V1(config) => MainNodeFeeParams::V1(MainNodeFeeParamsV1 {
                config,
                l1_gas_price: self.provider.estimate_effective_gas_price(),
            }),
            MainNodeFeeModelConfig::V2(config) => MainNodeFeeParams::V2(MainNodeFeeParamsV2 {
                config,
                l1_gas_price: self.provider.estimate_effective_gas_price(),
                l1_pubdata_price: self.provider.estimate_effective_pubdata_price(),
            }),
        }
    }
}

impl<G: L1GasPriceProvider + ?Sized> MainNodeFeeInputProvider<G> {
    pub(crate) fn new(provider: Arc<G>, config: MainNodeFeeModelConfig) -> Self {
        Self { provider, config }
    }
}

/// Calculates the batch fee input based on the main node parameters.
/// This function uses the `V1` fee model, i.e. where the pubdata price does not include the proving costs.
pub(crate) fn compute_batch_fee_model_input_v1(
    params: MainNodeFeeParamsV1,
    l1_gas_price_scale_factor: f64,
) -> BatchFeeInput {
    let l1_gas_price = (params.l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;

    BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price: params.config.minimal_l2_gas_price,
    })
}

/// Calculates the batch fee input based on the main node parameters.
/// This function uses the `V2` fee model, i.e. where the pubdata price does not include the proving costs.
pub(crate) fn compute_batch_fee_model_input_v2(
    params: MainNodeFeeParamsV2,
    l1_gas_price_scale_factor: f64,
    l1_pubdata_price_scale_factor: f64,
) -> BatchFeeInput {
    let MainNodeFeeParamsV2 {
        config,
        l1_gas_price,
        l1_pubdata_price,
    } = params;

    let MainNodeFeeModelConfigV2 {
        minimal_l2_gas_price,
        compute_overhead_percent,
        pubdata_overhead_percent,
        batch_overhead_l1_gas,
        max_gas_per_batch,
        max_pubdata_per_batch,
    } = config;

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

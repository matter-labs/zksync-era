use std::{fmt, fmt::Debug, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    fee_model::{
        BaseTokenConversionRatio, BatchFeeInput, FeeModelConfig, FeeModelConfigV2, FeeParams,
        FeeParamsV1, FeeParamsV2, L1PeggedBatchFeeModelInput, PubdataIndependentBatchFeeModelInput,
    },
    U256,
};
use zksync_utils::ceil_div_u256;

use crate::l1_gas_price::GasAdjuster;

pub mod l1_gas_price;

/// Trait responsible for providing numerator and denominator for adjusting gas price that is denominated
/// in a non-eth base token
#[async_trait]
pub trait BaseTokenRatioProvider: Debug + Send + Sync + 'static {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio;
}

/// Trait responsible for providing fee info for a batch
#[async_trait]
pub trait BatchFeeModelInputProvider: fmt::Debug + 'static + Send + Sync {
    /// Returns the batch fee with scaling applied. This may be used to account for the fact that the L1 gas and pubdata prices may fluctuate, esp.
    /// in API methods that should return values that are valid for some period of time after the estimation was done.
    async fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        let params = self.get_fee_model_params();

        Ok(match params {
            FeeParams::V1(params) => BatchFeeInput::L1Pegged(compute_batch_fee_model_input_v1(
                params,
                l1_gas_price_scale_factor,
            )),
            FeeParams::V2(params) => BatchFeeInput::PubdataIndependent(
                clip_batch_fee_model_input_v2(compute_batch_fee_model_input_v2(
                    params,
                    l1_gas_price_scale_factor,
                    l1_pubdata_price_scale_factor,
                )),
            ),
        })
    }

    /// Returns the fee model parameters using the denomination of the base token used (WEI for ETH).
    fn get_fee_model_params(&self) -> FeeParams;
}

impl dyn BatchFeeModelInputProvider {
    /// Returns the batch fee input as-is, i.e. without any scaling for the L1 gas and pubdata prices.
    pub async fn get_batch_fee_input(&self) -> anyhow::Result<BatchFeeInput> {
        self.get_batch_fee_input_scaled(1.0, 1.0).await
    }
}

/// The struct that represents the batch fee input provider to be used in the main node of the server.
/// This struct gets the L1 gas price directly from the provider rather than from another node, as is the
/// case with the external node.
#[derive(Debug)]
pub struct MainNodeFeeInputProvider {
    provider: Arc<GasAdjuster>,
    base_token_ratio_provider: Arc<dyn BaseTokenRatioProvider>,
    config: FeeModelConfig,
}

#[async_trait]
impl BatchFeeModelInputProvider for MainNodeFeeInputProvider {
    fn get_fee_model_params(&self) -> FeeParams {
        match self.config {
            FeeModelConfig::V1(config) => FeeParams::V1(FeeParamsV1 {
                config,
                l1_gas_price: self.provider.estimate_effective_gas_price(),
            }),
            FeeModelConfig::V2(config) => FeeParams::V2(FeeParamsV2::new(
                config,
                self.provider.estimate_effective_gas_price(),
                self.provider.estimate_effective_pubdata_price(),
                self.base_token_ratio_provider.get_conversion_ratio(),
            )),
        }
    }
}

impl MainNodeFeeInputProvider {
    pub fn new(
        provider: Arc<GasAdjuster>,
        base_token_ratio_provider: Arc<dyn BaseTokenRatioProvider>,
        config: FeeModelConfig,
    ) -> Self {
        Self {
            provider,
            base_token_ratio_provider,
            config,
        }
    }
}

/// The fee model provider to be used in the API. It returns the maximum batch fee input between the projected main node one and
/// the one from the last sealed L2 block.
#[derive(Debug)]
pub struct ApiFeeInputProvider {
    inner: Arc<dyn BatchFeeModelInputProvider>,
    connection_pool: ConnectionPool<Core>,
}

impl ApiFeeInputProvider {
    pub fn new(
        inner: Arc<dyn BatchFeeModelInputProvider>,
        connection_pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            inner,
            connection_pool,
        }
    }
}

#[async_trait]
impl BatchFeeModelInputProvider for ApiFeeInputProvider {
    async fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        let inner_input = self
            .inner
            .get_batch_fee_input_scaled(l1_gas_price_scale_factor, l1_pubdata_price_scale_factor)
            .await
            .context("cannot get batch fee input from base provider")?;
        let last_l2_block_params = self
            .connection_pool
            .connection_tagged("api_fee_input_provider")
            .await?
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await?;

        Ok(last_l2_block_params
            .map(|header| inner_input.stricter(header.batch_fee_input))
            .unwrap_or(inner_input))
    }

    /// Returns the fee model parameters.
    fn get_fee_model_params(&self) -> FeeParams {
        self.inner.get_fee_model_params()
    }
}

/// Calculates the batch fee input based on the main node parameters.
/// This function uses the `V1` fee model, i.e. where the pubdata price does not include the proving costs.
fn compute_batch_fee_model_input_v1(
    params: FeeParamsV1,
    l1_gas_price_scale_factor: f64,
) -> L1PeggedBatchFeeModelInput {
    let l1_gas_price = (params.l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;

    L1PeggedBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price: params.config.minimal_l2_gas_price,
    }
}

/// Calculates the batch fee input based on the main node parameters.
/// This function uses the `V2` fee model, i.e. where the pubdata price does not include the proving costs.
fn compute_batch_fee_model_input_v2(
    params: FeeParamsV2,
    l1_gas_price_scale_factor: f64,
    l1_pubdata_price_scale_factor: f64,
) -> PubdataIndependentBatchFeeModelInput {
    let config = params.config();
    let l1_gas_price = params.l1_gas_price();
    let l1_pubdata_price = params.l1_pubdata_price();

    let FeeModelConfigV2 {
        minimal_l2_gas_price,
        compute_overhead_part,
        pubdata_overhead_part,
        batch_overhead_l1_gas,
        max_gas_per_batch,
        max_pubdata_per_batch,
    } = config;

    // Firstly, we scale the gas price and pubdata price in case it is needed.
    let l1_gas_price = (l1_gas_price as f64 * l1_gas_price_scale_factor) as u64;
    let l1_pubdata_price = (l1_pubdata_price as f64 * l1_pubdata_price_scale_factor) as u64;

    // While the final results of the calculations are not expected to have any overflows, the intermediate computations
    // might, so we use U256 for them.
    let l1_batch_overhead_wei = U256::from(l1_gas_price) * U256::from(batch_overhead_l1_gas);

    let fair_l2_gas_price = {
        // Firstly, we calculate which part of the overall overhead each unit of L2 gas should cover.
        let l1_batch_overhead_per_gas =
            ceil_div_u256(l1_batch_overhead_wei, U256::from(max_gas_per_batch));

        // Then, we multiply by the `compute_overhead_part` to get the overhead for the computation for each gas.
        // Also, this means that if we almost never close batches because of compute, the `compute_overhead_part` should be zero and so
        // it is possible that the computation costs include for no overhead.
        let gas_overhead_wei =
            (l1_batch_overhead_per_gas.as_u64() as f64 * compute_overhead_part) as u64;

        // We sum up the minimal L2 gas price (i.e. the raw prover/compute cost of a single L2 gas) and the overhead for batch being closed.
        minimal_l2_gas_price + gas_overhead_wei
    };

    let fair_pubdata_price = {
        // Firstly, we calculate which part of the overall overhead each pubdata byte should cover.
        let l1_batch_overhead_per_pubdata =
            ceil_div_u256(l1_batch_overhead_wei, U256::from(max_pubdata_per_batch));

        // Then, we multiply by the `pubdata_overhead_part` to get the overhead for each pubdata byte.
        // Also, this means that if we almost never close batches because of pubdata, the `pubdata_overhead_part` should be zero and so
        // it is possible that the pubdata costs include no overhead.
        let pubdata_overhead_wei =
            (l1_batch_overhead_per_pubdata.as_u64() as f64 * pubdata_overhead_part) as u64;

        // We sum up the raw L1 pubdata price (i.e. the expected price of publishing a single pubdata byte) and the overhead for batch being closed.
        l1_pubdata_price + pubdata_overhead_wei
    };

    PubdataIndependentBatchFeeModelInput {
        l1_gas_price,
        fair_l2_gas_price,
        fair_pubdata_price,
    }
}

/// Bootloader places limitations on fair_l2_gas_price and fair_pubdata_price.
/// (MAX_ALLOWED_FAIR_L2_GAS_PRICE and MAX_ALLOWED_FAIR_PUBDATA_PRICE in bootloader code respectively)
/// Server needs to clip this prices in order to allow chain continues operation at a loss. The alternative
/// would be to stop accepting the transactions until the conditions improve.
/// TODO (PE-153): to be removed when bootloader limitation is removed
fn clip_batch_fee_model_input_v2(
    fee_model: PubdataIndependentBatchFeeModelInput,
) -> PubdataIndependentBatchFeeModelInput {
    /// MAX_ALLOWED_FAIR_L2_GAS_PRICE
    const MAXIMUM_L2_GAS_PRICE: u64 = 10_000_000_000_000;
    /// MAX_ALLOWED_FAIR_PUBDATA_PRICE
    const MAXIMUM_PUBDATA_PRICE: u64 = 1_000_000_000_000_000;
    PubdataIndependentBatchFeeModelInput {
        l1_gas_price: fee_model.l1_gas_price,
        fair_l2_gas_price: if fee_model.fair_l2_gas_price < MAXIMUM_L2_GAS_PRICE {
            fee_model.fair_l2_gas_price
        } else {
            tracing::warn!(
                "Fair l2 gas price {} exceeds maximum. Limitting to {}",
                fee_model.fair_l2_gas_price,
                MAXIMUM_L2_GAS_PRICE
            );
            MAXIMUM_L2_GAS_PRICE
        },
        fair_pubdata_price: if fee_model.fair_pubdata_price < MAXIMUM_PUBDATA_PRICE {
            fee_model.fair_pubdata_price
        } else {
            tracing::warn!(
                "Fair pubdata price {} exceeds maximum. Limitting to {}",
                fee_model.fair_pubdata_price,
                MAXIMUM_PUBDATA_PRICE
            );
            MAXIMUM_PUBDATA_PRICE
        },
    }
}

/// Mock [`BatchFeeModelInputProvider`] implementation that returns a constant value.
/// Intended to be used in tests only.
#[derive(Debug)]
pub struct MockBatchFeeParamsProvider(pub FeeParams);

impl Default for MockBatchFeeParamsProvider {
    fn default() -> Self {
        Self(FeeParams::sensible_v1_default())
    }
}

#[async_trait]
impl BatchFeeModelInputProvider for MockBatchFeeParamsProvider {
    fn get_fee_model_params(&self) -> FeeParams {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use zksync_config::{configs::eth_sender::PubdataSendingMode, GasAdjusterConfig};
    use zksync_eth_client::{clients::MockEthereum, BaseFees};
    use zksync_types::{commitment::L1BatchCommitmentMode, fee_model::BaseTokenConversionRatio};

    use super::*;

    // To test that overflow never happens, we'll use giant L1 gas price, i.e.
    // almost realistic very large value of 100k gwei. Since it is so large, we'll also
    // use it for the L1 pubdata price.
    const GWEI: u64 = 1_000_000_000;
    const GIANT_L1_GAS_PRICE: u64 = 100_000 * GWEI;

    // As a small L2 gas price we'll use the value of 1 wei.
    const SMALL_L1_GAS_PRICE: u64 = 1;

    #[test]
    fn test_compute_batch_fee_model_input_v2_giant_numbers() {
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: GIANT_L1_GAS_PRICE,
            // We generally don't expect those values to be larger than 1. Still, in theory the operator
            // may need to set higher values in extreme cases.
            compute_overhead_part: 5.0,
            pubdata_overhead_part: 5.0,
            // The batch overhead would likely never grow beyond that
            batch_overhead_l1_gas: 1_000_000,
            // Let's imagine that for some reason the limit is relatively small
            max_gas_per_batch: 50_000_000,
            // The pubdata will likely never go below that
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2::new(
            config,
            GIANT_L1_GAS_PRICE,
            GIANT_L1_GAS_PRICE,
            BaseTokenConversionRatio::default(),
        );

        // We'll use scale factor of 3.0
        let input = compute_batch_fee_model_input_v2(params, 3.0, 3.0);

        assert_eq!(input.l1_gas_price, GIANT_L1_GAS_PRICE * 3);
        assert_eq!(input.fair_l2_gas_price, 130_000_000_000_000);
        assert_eq!(input.fair_pubdata_price, 15_300_000_000_000_000);
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_small_numbers() {
        // Here we assume that the operator wants to make the lives of users as cheap as possible.
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: SMALL_L1_GAS_PRICE,
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 0.0,
            batch_overhead_l1_gas: 0,
            max_gas_per_batch: 50_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2::new(
            config,
            SMALL_L1_GAS_PRICE,
            SMALL_L1_GAS_PRICE,
            BaseTokenConversionRatio::default(),
        );

        let input =
            clip_batch_fee_model_input_v2(compute_batch_fee_model_input_v2(params, 1.0, 1.0));

        assert_eq!(input.l1_gas_price, SMALL_L1_GAS_PRICE);
        assert_eq!(input.fair_l2_gas_price, SMALL_L1_GAS_PRICE);
        assert_eq!(input.fair_pubdata_price, SMALL_L1_GAS_PRICE);
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_only_pubdata_overhead() {
        // Here we use sensible config, but when only pubdata is used to close the batch
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: 100_000_000_000,
            compute_overhead_part: 0.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2::new(
            config,
            GIANT_L1_GAS_PRICE,
            GIANT_L1_GAS_PRICE,
            BaseTokenConversionRatio::default(),
        );

        let input =
            clip_batch_fee_model_input_v2(compute_batch_fee_model_input_v2(params, 1.0, 1.0));
        assert_eq!(input.l1_gas_price, GIANT_L1_GAS_PRICE);
        // The fair L2 gas price is identical to the minimal one.
        assert_eq!(input.fair_l2_gas_price, 100_000_000_000);
        // The fair pubdata price is the minimal one plus the overhead.
        assert_eq!(input.fair_pubdata_price, 800_000_000_000_000);
    }

    #[test]
    fn test_compute_baxtch_fee_model_input_v2_only_compute_overhead() {
        // Here we use sensible config, but when only compute is used to close the batch
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: 100_000_000_000,
            compute_overhead_part: 1.0,
            pubdata_overhead_part: 0.0,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2::new(
            config,
            GIANT_L1_GAS_PRICE,
            GIANT_L1_GAS_PRICE,
            BaseTokenConversionRatio::default(),
        );

        let input = compute_batch_fee_model_input_v2(params, 1.0, 1.0);
        assert_eq!(input.l1_gas_price, GIANT_L1_GAS_PRICE);
        // The fair L2 gas price is identical to the minimal one, plus the overhead
        assert_eq!(input.fair_l2_gas_price, 240_000_000_000);
        // The fair pubdata price is equal to the original one.
        assert_eq!(input.fair_pubdata_price, GIANT_L1_GAS_PRICE);
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_param_tweaking() {
        // In this test we generally checking that each param behaves as expected
        let base_config = FeeModelConfigV2 {
            minimal_l2_gas_price: 100_000_000_000,
            compute_overhead_part: 0.5,
            pubdata_overhead_part: 0.5,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let base_params = FeeParamsV2::new(
            base_config,
            1_000_000_000,
            1_000_000_000,
            BaseTokenConversionRatio::default(),
        );

        let base_input = compute_batch_fee_model_input_v2(base_params, 1.0, 1.0);

        let base_input_larger_l1_gas_price = compute_batch_fee_model_input_v2(
            FeeParamsV2::new(
                base_config,
                2_000_000_000, // double the L1 gas price
                1_000_000_000,
                BaseTokenConversionRatio::default(),
            ),
            1.0,
            1.0,
        );
        let base_input_scaled_l1_gas_price =
            compute_batch_fee_model_input_v2(base_params, 2.0, 1.0);
        assert_eq!(
            base_input_larger_l1_gas_price, base_input_scaled_l1_gas_price,
            "Scaling has the correct effect for the L1 gas price"
        );
        assert!(
            base_input.fair_l2_gas_price < base_input_larger_l1_gas_price.fair_l2_gas_price,
            "L1 gas price increase raises L2 gas price"
        );
        assert!(
            base_input.fair_pubdata_price < base_input_larger_l1_gas_price.fair_pubdata_price,
            "L1 gas price increase raises pubdata price"
        );

        let base_input_larger_pubdata_price = compute_batch_fee_model_input_v2(
            FeeParamsV2::new(
                base_config,
                1_000_000_000,
                2_000_000_000, // double the L1 pubdata price
                BaseTokenConversionRatio::default(),
            ),
            1.0,
            1.0,
        );
        let base_input_scaled_pubdata_price =
            compute_batch_fee_model_input_v2(base_params, 1.0, 2.0);
        assert_eq!(
            base_input_larger_pubdata_price, base_input_scaled_pubdata_price,
            "Scaling has the correct effect for the pubdata price"
        );
        assert_eq!(
            base_input.fair_l2_gas_price, base_input_larger_pubdata_price.fair_l2_gas_price,
            "L1 pubdata increase has no effect on L2 gas price"
        );
        assert!(
            base_input.fair_pubdata_price < base_input_larger_pubdata_price.fair_pubdata_price,
            "Pubdata price increase raises pubdata price"
        );

        let base_input_larger_max_gas = compute_batch_fee_model_input_v2(
            FeeParamsV2::new(
                FeeModelConfigV2 {
                    max_gas_per_batch: base_config.max_gas_per_batch * 2,
                    ..base_config
                },
                base_params.l1_gas_price(),
                base_params.l1_pubdata_price(),
                BaseTokenConversionRatio::default(),
            ),
            1.0,
            1.0,
        );
        assert!(
            base_input.fair_l2_gas_price > base_input_larger_max_gas.fair_l2_gas_price,
            "Max gas increase lowers L2 gas price"
        );
        assert_eq!(
            base_input.fair_pubdata_price, base_input_larger_max_gas.fair_pubdata_price,
            "Max gas increase has no effect on pubdata price"
        );

        let base_input_larger_max_pubdata = compute_batch_fee_model_input_v2(
            FeeParamsV2::new(
                FeeModelConfigV2 {
                    max_pubdata_per_batch: base_config.max_pubdata_per_batch * 2,
                    ..base_config
                },
                base_params.l1_gas_price(),
                base_params.l1_pubdata_price(),
                BaseTokenConversionRatio::default(),
            ),
            1.0,
            1.0,
        );
        assert_eq!(
            base_input.fair_l2_gas_price, base_input_larger_max_pubdata.fair_l2_gas_price,
            "Max pubdata increase has no effect on L2 gas price"
        );
        assert!(
            base_input.fair_pubdata_price > base_input_larger_max_pubdata.fair_pubdata_price,
            "Max pubdata increase lowers pubdata price"
        );
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_gas_price_over_limit_due_to_l1_gas() {
        // In this test we check the gas price limit works as expected
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: 100 * GWEI,
            compute_overhead_part: 0.5,
            pubdata_overhead_part: 0.5,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let l1_gas_price = 1_000_000_000 * GWEI;
        let params = FeeParamsV2::new(
            config,
            l1_gas_price,
            GIANT_L1_GAS_PRICE,
            BaseTokenConversionRatio::default(),
        );

        let input =
            clip_batch_fee_model_input_v2(compute_batch_fee_model_input_v2(params, 1.0, 1.0));
        assert_eq!(input.l1_gas_price, l1_gas_price);
        // The fair L2 gas price is identical to the maximum
        assert_eq!(input.fair_l2_gas_price, 10_000 * GWEI);
        assert_eq!(input.fair_pubdata_price, 1_000_000 * GWEI);
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_gas_price_over_limit_due_to_conversion_rate() {
        // In this test we check the gas price limit works as expected
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: GWEI,
            compute_overhead_part: 0.5,
            pubdata_overhead_part: 0.5,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2::new(
            config,
            GWEI,
            2 * GWEI,
            BaseTokenConversionRatio {
                numerator: NonZeroU64::new(3_000_000).unwrap(),
                denominator: NonZeroU64::new(1).unwrap(),
            },
        );

        let input =
            clip_batch_fee_model_input_v2(compute_batch_fee_model_input_v2(params, 1.0, 1.0));
        assert_eq!(input.l1_gas_price, 3_000_000 * GWEI);
        // The fair L2 gas price is identical to the maximum
        assert_eq!(input.fair_l2_gas_price, 10_000 * GWEI);
        assert_eq!(input.fair_pubdata_price, 1_000_000 * GWEI);
    }

    #[derive(Debug, Clone)]
    struct DummyTokenRatioProvider {
        ratio: BaseTokenConversionRatio,
    }

    impl DummyTokenRatioProvider {
        pub fn new(ratio: BaseTokenConversionRatio) -> Self {
            Self { ratio }
        }
    }

    #[async_trait]
    impl BaseTokenRatioProvider for DummyTokenRatioProvider {
        fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
            self.ratio
        }
    }

    #[tokio::test]
    async fn test_get_fee_model_params() {
        struct TestCase {
            name: &'static str,
            conversion_ratio: BaseTokenConversionRatio,
            input_minimal_l2_gas_price: u64,    // Wei denomination
            input_l1_gas_price: u64,            // Wei
            input_l1_pubdata_price: u64,        // Wei
            expected_minimal_l2_gas_price: u64, // BaseToken denomination
            expected_l1_gas_price: u64,         // BaseToken
            expected_l1_pubdata_price: u64,     // BaseToken
        }
        let test_cases = vec![
            TestCase {
                name: "1 ETH = 2 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(2).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 2000,
                expected_l1_gas_price: 4000,
                expected_l1_pubdata_price: 6000,
            },
            TestCase {
                name: "1 ETH = 0.5 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(2).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 500,
                expected_l1_gas_price: 1000,
                expected_l1_pubdata_price: 1500,
            },
            TestCase {
                name: "1 ETH = 1 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 1000,
                expected_l1_gas_price: 2000,
                expected_l1_pubdata_price: 3000,
            },
            TestCase {
                name: "Large conversion - 1 ETH = 1_000_000 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1_000_000).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1_000_000_000_000,
                expected_l1_gas_price: 2_000_000_000_000,
                expected_l1_pubdata_price: 3_000_000_000_000,
            },
            TestCase {
                name: "Small conversion - 1 ETH = 0.001 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(1_000).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1_000,
                expected_l1_gas_price: 2_000,
                expected_l1_pubdata_price: 3_000,
            },
            TestCase {
                name: "Fractional conversion ratio 123456789",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1123456789).unwrap(),
                    denominator: NonZeroU64::new(1_000_000_000).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1123456,
                expected_l1_gas_price: 2246913,
                expected_l1_pubdata_price: 3370370,
            },
            TestCase {
                name: "Conversion ratio too large so clamp down to u64::MAX",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(u64::MAX).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 2,
                input_l1_gas_price: 2,
                input_l1_pubdata_price: 2,
                expected_minimal_l2_gas_price: u64::MAX,
                expected_l1_gas_price: u64::MAX,
                expected_l1_pubdata_price: u64::MAX,
            },
        ];

        for case in test_cases {
            let gas_adjuster =
                setup_gas_adjuster(case.input_l1_gas_price, case.input_l1_pubdata_price).await;

            let base_token_ratio_provider = DummyTokenRatioProvider::new(case.conversion_ratio);

            let config = FeeModelConfig::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: case.input_minimal_l2_gas_price,
                compute_overhead_part: 1.0,
                pubdata_overhead_part: 1.0,
                batch_overhead_l1_gas: 1,
                max_gas_per_batch: 1,
                max_pubdata_per_batch: 1,
            });

            let fee_provider = MainNodeFeeInputProvider::new(
                Arc::new(gas_adjuster),
                Arc::new(base_token_ratio_provider),
                config,
            );

            let fee_params = fee_provider.get_fee_model_params();

            if let FeeParams::V2(params) = fee_params {
                assert_eq!(
                    params.l1_gas_price(),
                    case.expected_l1_gas_price,
                    "Test case '{}' failed: l1_gas_price mismatch",
                    case.name
                );
                assert_eq!(
                    params.l1_pubdata_price(),
                    case.expected_l1_pubdata_price,
                    "Test case '{}' failed: l1_pubdata_price mismatch",
                    case.name
                );
                assert_eq!(
                    params.config().minimal_l2_gas_price,
                    case.expected_minimal_l2_gas_price,
                    "Test case '{}' failed: minimal_l2_gas_price mismatch",
                    case.name
                );
            } else {
                panic!("Expected FeeParams::V2 for test case '{}'", case.name);
            }
        }
    }

    // Helper function to create BaseFees.
    fn base_fees(block: u64, blob: U256) -> BaseFees {
        BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: blob,
        }
    }

    // Helper function to setup the GasAdjuster.
    async fn setup_gas_adjuster(l1_gas_price: u64, l1_pubdata_price: u64) -> GasAdjuster {
        let mock = MockEthereum::builder()
            .with_fee_history(vec![
                base_fees(0, U256::from(4)),
                base_fees(1, U256::from(3)),
            ])
            .build();
        mock.advance_block_number(2); // Ensure we have enough blocks for the fee history

        let gas_adjuster_config = GasAdjusterConfig {
            internal_enforced_l1_gas_price: Some(l1_gas_price),
            internal_enforced_pubdata_price: Some(l1_pubdata_price),
            max_base_fee_samples: 1, // Ensure this is less than the number of blocks
            num_samples_for_blob_base_fee_estimate: 2,
            ..Default::default()
        };

        GasAdjuster::new(
            Box::new(mock.into_client()),
            gas_adjuster_config,
            PubdataSendingMode::Blobs,
            L1BatchCommitmentMode::Rollup,
        )
        .await
        .expect("Failed to create GasAdjuster")
    }
}

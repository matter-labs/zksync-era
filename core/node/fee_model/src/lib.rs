use std::{fmt, sync::Arc};

use anyhow::Context as _;
use bigdecimal::{BigDecimal, ToPrimitive};
use zksync_base_token_adjuster::BaseTokenAdjuster;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    fee_model::{
        BatchFeeInput, FeeModelConfig, FeeModelConfigV2, FeeParams, FeeParamsV1, FeeParamsV2,
        L1PeggedBatchFeeModelInput, PubdataIndependentBatchFeeModelInput,
    },
    U256,
};
use zksync_utils::ceil_div_u256;

use crate::l1_gas_price::L1GasAdjuster;

pub mod l1_gas_price;

/// Trait responsible for providing fee info for a batch
#[async_trait::async_trait]
pub trait BatchFeeModelInputProvider: fmt::Debug + 'static + Send + Sync {
    /// Returns the batch fee with scaling applied. This may be used to account for the fact that the L1 gas and pubdata prices may fluctuate, esp.
    /// in API methods that should return values that are valid for some period of time after the estimation was done.
    async fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        let params = self.get_fee_model_params().await?;

        Ok(match params {
            FeeParams::V1(params) => BatchFeeInput::L1Pegged(compute_batch_fee_model_input_v1(
                params,
                l1_gas_price_scale_factor,
            )),
            FeeParams::V2(params) => {
                BatchFeeInput::PubdataIndependent(compute_batch_fee_model_input_v2(
                    params,
                    l1_gas_price_scale_factor,
                    l1_pubdata_price_scale_factor,
                ))
            }
        })
    }

    /// Returns the fee model parameters using the denomination of the base token used (WEI for ETH).
    async fn get_fee_model_params(&self) -> anyhow::Result<FeeParams>;
}

impl dyn BatchFeeModelInputProvider {
    /// Returns the batch fee input as-is, i.e. without any scaling for the L1 gas and pubdata prices.
    pub async fn get_batch_fee_input(&self) -> anyhow::Result<BatchFeeInput> {
        self.get_batch_fee_input_scaled(1.0, 1.0).await
    }
}

/// The struct that represents the batch fee input provider to be used in the main node of the server, i.e.
/// it explicitly gets the L1 gas price from the provider and uses it to calculate the batch fee input instead of getting
/// it from other node.
#[derive(Debug)]
pub struct MainNodeFeeInputProvider {
    provider: Arc<dyn L1GasAdjuster>,
    base_token_adjuster: Arc<dyn BaseTokenAdjuster>,
    config: FeeModelConfig,
}

#[async_trait::async_trait]
impl BatchFeeModelInputProvider for MainNodeFeeInputProvider {
    async fn get_fee_model_params(&self) -> anyhow::Result<FeeParams> {
        match self.config {
            FeeModelConfig::V1(config) => Ok(FeeParams::V1(FeeParamsV1 {
                config,
                l1_gas_price: self.provider.estimate_effective_gas_price(),
            })),
            FeeModelConfig::V2(config) => {
                let params = FeeParamsV2 {
                    config,
                    l1_gas_price: self.provider.estimate_effective_gas_price(),
                    l1_pubdata_price: self.provider.estimate_effective_pubdata_price(),
                };

                let base_token = self.base_token_adjuster.get_base_token();
                match base_token {
                    "ETH" => Ok(FeeParams::V2(params)),
                    _ => {
                        let base_token_conversion_ratio = self
                            .base_token_adjuster
                            .get_last_ratio_and_check_usability()
                            .await;
                        Ok(FeeParams::V2(convert_to_base_token(
                            params,
                            base_token_conversion_ratio,
                        )))
                    }
                }
            }
        }
    }
}

impl MainNodeFeeInputProvider {
    pub fn new(
        provider: Arc<dyn L1GasAdjuster>,
        base_token_adjuster: Arc<dyn BaseTokenAdjuster>,
        config: FeeModelConfig,
    ) -> Self {
        Self {
            provider,
            base_token_adjuster,
            config,
        }
    }
}

fn convert_to_base_token(
    params: FeeParamsV2,
    base_token_conversion_ratio: BigDecimal,
) -> FeeParamsV2 {
    let FeeParamsV2 {
        config,
        l1_gas_price,
        l1_pubdata_price,
    } = params;

    let convert_price = |price_in_wei: u64, eth_to_base_token_ratio: &BigDecimal| -> u64 {
        let converted_price_bd = BigDecimal::from(price_in_wei) * eth_to_base_token_ratio;
        match converted_price_bd.to_u64() {
            Some(converted_price) => converted_price,
            None => {
                if converted_price_bd > BigDecimal::from(u64::MAX) {
                    tracing::warn!(
                        "Conversion to base token price failed: converted price is too large: {}",
                        converted_price_bd
                    );
                } else {
                    tracing::error!(
                        "Conversion to base token price failed: converted price is not a valid u64: {}",
                        converted_price_bd
                    );
                }
                u64::MAX
            }
        }
    };

    let l1_gas_price_converted = convert_price(l1_gas_price, &base_token_conversion_ratio);
    let l1_pubdata_price_converted = convert_price(l1_pubdata_price, &base_token_conversion_ratio);
    let minimal_l2_gas_price_converted =
        convert_price(config.minimal_l2_gas_price, &base_token_conversion_ratio);

    FeeParamsV2 {
        config: FeeModelConfigV2 {
            minimal_l2_gas_price: minimal_l2_gas_price_converted,
            ..config
        },
        l1_gas_price: l1_gas_price_converted,
        l1_pubdata_price: l1_pubdata_price_converted,
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

#[async_trait::async_trait]
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
    async fn get_fee_model_params(&self) -> anyhow::Result<FeeParams> {
        self.inner.get_fee_model_params().await
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
    let FeeParamsV2 {
        config,
        l1_gas_price,
        l1_pubdata_price,
    } = params;

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
        // Firstly, we calculate which part of the overall overhead overhead each unit of L2 gas should cover.
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
        // Firstly, we calculate which part of the overall overhead overhead each pubdata byte should cover.
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

/// Mock [`BatchFeeModelInputProvider`] implementation that returns a constant value.
/// Intended to be used in tests only.
#[derive(Debug)]
pub struct MockBatchFeeParamsProvider(pub FeeParams);

impl Default for MockBatchFeeParamsProvider {
    fn default() -> Self {
        Self(FeeParams::sensible_v1_default())
    }
}

#[async_trait::async_trait]
impl BatchFeeModelInputProvider for MockBatchFeeParamsProvider {
    async fn get_fee_model_params<'a>(&'a self) -> anyhow::Result<FeeParams> {
        Ok(self.0)
    }
}

#[cfg(test)]
mod tests {
    use bigdecimal::FromPrimitive;
    use test_casing::test_casing;
    use zksync_base_token_adjuster::MockBaseTokenAdjuster;

    use super::*;
    use crate::l1_gas_price::MockGasAdjuster;

    // To test that overflow never happens, we'll use giant L1 gas price, i.e.
    // almost realistic very large value of 100k gwei. Since it is so large, we'll also
    // use it for the L1 pubdata price.
    const GIANT_L1_GAS_PRICE: u64 = 100_000_000_000_000;

    // As a small small L2 gas price we'll use the value of 1 wei.
    const SMALL_L1_GAS_PRICE: u64 = 1;

    // Conversion ratio for ETH to base token (1ETH = 200K BaseToken)
    const ETH_TO_BASE_TOKEN: u64 = 200000;

    // Conversion ratio for ETH to ETH (1ETH = 1ETH)
    const ETH_TO_ETH: u64 = 1;

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

        let params = FeeParamsV2 {
            config,
            l1_gas_price: GIANT_L1_GAS_PRICE,
            l1_pubdata_price: GIANT_L1_GAS_PRICE,
        };

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

        let params = FeeParamsV2 {
            config,
            l1_gas_price: SMALL_L1_GAS_PRICE,
            l1_pubdata_price: SMALL_L1_GAS_PRICE,
        };

        let input = compute_batch_fee_model_input_v2(params, 1.0, 1.0);

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

        let params = FeeParamsV2 {
            config,
            l1_gas_price: GIANT_L1_GAS_PRICE,
            l1_pubdata_price: GIANT_L1_GAS_PRICE,
        };

        let input = compute_batch_fee_model_input_v2(params, 1.0, 1.0);
        assert_eq!(input.l1_gas_price, GIANT_L1_GAS_PRICE);
        // The fair L2 gas price is identical to the minimal one.
        assert_eq!(input.fair_l2_gas_price, 100_000_000_000);
        // The fair pubdata price is the minimal one plus the overhead.
        assert_eq!(input.fair_pubdata_price, 800_000_000_000_000);
    }

    #[test]
    fn test_compute_batch_fee_model_input_v2_only_compute_overhead() {
        // Here we use sensible config, but when only compute is used to close the batch
        let config = FeeModelConfigV2 {
            minimal_l2_gas_price: 100_000_000_000,
            compute_overhead_part: 1.0,
            pubdata_overhead_part: 0.0,
            batch_overhead_l1_gas: 700_000,
            max_gas_per_batch: 500_000_000,
            max_pubdata_per_batch: 100_000,
        };

        let params = FeeParamsV2 {
            config,
            l1_gas_price: GIANT_L1_GAS_PRICE,
            l1_pubdata_price: GIANT_L1_GAS_PRICE,
        };

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

        let base_params = FeeParamsV2 {
            config: base_config,
            l1_gas_price: 1_000_000_000,
            l1_pubdata_price: 1_000_000_000,
        };

        let base_input = compute_batch_fee_model_input_v2(base_params, 1.0, 1.0);

        let base_input_larger_l1_gas_price = compute_batch_fee_model_input_v2(
            FeeParamsV2 {
                l1_gas_price: base_params.l1_gas_price * 2,
                ..base_params
            },
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
            FeeParamsV2 {
                l1_pubdata_price: base_params.l1_pubdata_price * 2,
                ..base_params
            },
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
            FeeParamsV2 {
                config: FeeModelConfigV2 {
                    max_gas_per_batch: base_config.max_gas_per_batch * 2,
                    ..base_config
                },
                ..base_params
            },
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
            FeeParamsV2 {
                config: FeeModelConfigV2 {
                    max_pubdata_per_batch: base_config.max_pubdata_per_batch * 2,
                    ..base_config
                },
                ..base_params
            },
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

    #[test_casing(2, [("ETH", ETH_TO_ETH),("ZK", ETH_TO_BASE_TOKEN)])]
    #[tokio::test]
    async fn test_get_fee_model_params(base_token: &str, conversion_ratio: u64) {
        let conversion_ratio_bd = BigDecimal::from_u64(conversion_ratio).unwrap();
        let in_effective_l1_gas_price = 10_000_000_000; // 10 gwei
        let in_effective_l1_pubdata_price = 20_000_000; // 0.002 gwei
        let in_minimal_l2_gas_price = 25_000_000; // 0.025 gwei

        let gas_adjuster = Arc::new(MockGasAdjuster::new(
            in_effective_l1_gas_price,
            in_effective_l1_pubdata_price,
        ));

        let base_token_adjuster = Arc::new(MockBaseTokenAdjuster::new(
            conversion_ratio_bd.clone(),
            base_token.to_string(),
        ));

        let config = FeeModelConfig::V2(FeeModelConfigV2 {
            minimal_l2_gas_price: in_minimal_l2_gas_price,
            compute_overhead_part: 1.0,
            pubdata_overhead_part: 1.0,
            batch_overhead_l1_gas: 1_000_000,
            max_gas_per_batch: 50_000_000,
            max_pubdata_per_batch: 100_000,
        });

        let fee_provider = MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            base_token_adjuster.clone(),
            config,
        );

        let fee_params = fee_provider.get_fee_model_params().await.unwrap();

        let expected_l1_gas_price = (BigDecimal::from(in_effective_l1_gas_price)
            * conversion_ratio_bd.clone())
        .to_u64()
        .unwrap();
        let expected_l1_pubdata_price = (BigDecimal::from(in_effective_l1_pubdata_price)
            * conversion_ratio_bd.clone())
        .to_u64()
        .unwrap();
        let expected_minimal_l2_gas_price = (BigDecimal::from(in_minimal_l2_gas_price)
            * conversion_ratio_bd)
            .to_u64()
            .unwrap();

        if let FeeParams::V2(params) = fee_params {
            assert_eq!(params.l1_gas_price, expected_l1_gas_price);
            assert_eq!(params.l1_pubdata_price, expected_l1_pubdata_price);
            assert_eq!(
                params.config.minimal_l2_gas_price,
                expected_minimal_l2_gas_price
            );
        } else {
            panic!("Expected FeeParams::V2");
        }
    }
}

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

use crate::{ceil_div_u256, ProtocolVersionId, U256};

/// Fee input to be provided into the VM. It contains two options:
/// - `L1Pegged`: L1 gas price is provided to the VM, and the pubdata price is derived from it. Using this option is required for the
///   versions of Era prior to 1.4.1 integration.
/// - `PubdataIndependent`: L1 gas price and pubdata price are not necessarily dependent on one another. This options is more suitable for the
///   versions of Era after the 1.4.1 integration. It is expected that if a VM supports `PubdataIndependent` version, then it should also support
///   `L1Pegged` version, but converting it into `PubdataIndependentBatchFeeModelInput` in-place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchFeeInput {
    L1Pegged(L1PeggedBatchFeeModelInput),
    PubdataIndependent(PubdataIndependentBatchFeeModelInput),
}

impl BatchFeeInput {
    // Sometimes for temporary usage or tests a "sensible" default, i.e. the one consisting of non-zero values is needed.
    pub fn sensible_l1_pegged_default() -> Self {
        Self::L1Pegged(L1PeggedBatchFeeModelInput {
            l1_gas_price: 1_000_000_000,
            fair_l2_gas_price: 100_000_000,
        })
    }

    pub fn l1_pegged(l1_gas_price: u64, fair_l2_gas_price: u64) -> Self {
        Self::L1Pegged(L1PeggedBatchFeeModelInput {
            l1_gas_price,
            fair_l2_gas_price,
        })
    }

    pub fn pubdata_independent(
        l1_gas_price: u64,
        fair_l2_gas_price: u64,
        fair_pubdata_price: u64,
    ) -> Self {
        Self::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            l1_gas_price,
            fair_l2_gas_price,
            fair_pubdata_price,
        })
    }

    pub fn from_protocol_version(
        protocol_version: Option<ProtocolVersionId>,
        l1_gas_price: u64,
        fair_l2_gas_price: u64,
        fair_pubdata_price: Option<u64>,
    ) -> Self {
        protocol_version
            .filter(|version: &ProtocolVersionId| version.is_post_1_4_1())
            .map(|_| {
                Self::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
                    fair_pubdata_price: fair_pubdata_price
                        .expect("No fair pubdata price for 1.4.1"),
                    fair_l2_gas_price,
                    l1_gas_price,
                })
            })
            .unwrap_or_else(|| {
                Self::L1Pegged(L1PeggedBatchFeeModelInput {
                    fair_l2_gas_price,
                    l1_gas_price,
                })
            })
    }

    pub fn scale_fair_l2_gas_price(&self, scale_factor: f64) -> BatchFeeInput {
        match self {
            BatchFeeInput::L1Pegged(input) => {
                BatchFeeInput::L1Pegged(input.scale_fair_l2_gas_price(scale_factor))
            }
            BatchFeeInput::PubdataIndependent(input) => {
                BatchFeeInput::PubdataIndependent(input.scale_fair_l2_gas_price(scale_factor))
            }
        }
    }
}

impl Default for BatchFeeInput {
    fn default() -> Self {
        Self::L1Pegged(L1PeggedBatchFeeModelInput {
            l1_gas_price: 0,
            fair_l2_gas_price: 0,
        })
    }
}

impl BatchFeeInput {
    pub fn into_l1_pegged(self) -> L1PeggedBatchFeeModelInput {
        match self {
            BatchFeeInput::L1Pegged(input) => input,
            _ => panic!("Can not convert PubdataIndependentBatchFeeModelInput into L1PeggedBatchFeeModelInput"),
        }
    }

    pub fn fair_pubdata_price(&self) -> u64 {
        match self {
            BatchFeeInput::L1Pegged(input) => input.l1_gas_price * L1_GAS_PER_PUBDATA_BYTE as u64,
            BatchFeeInput::PubdataIndependent(input) => input.fair_pubdata_price,
        }
    }

    pub fn fair_l2_gas_price(&self) -> u64 {
        match self {
            BatchFeeInput::L1Pegged(input) => input.fair_l2_gas_price,
            BatchFeeInput::PubdataIndependent(input) => input.fair_l2_gas_price,
        }
    }

    pub fn l1_gas_price(&self) -> u64 {
        match self {
            BatchFeeInput::L1Pegged(input) => input.l1_gas_price,
            BatchFeeInput::PubdataIndependent(input) => input.l1_gas_price,
        }
    }

    pub fn into_pubdata_independent(self) -> PubdataIndependentBatchFeeModelInput {
        match self {
            BatchFeeInput::PubdataIndependent(input) => input,
            BatchFeeInput::L1Pegged(input) => PubdataIndependentBatchFeeModelInput {
                fair_l2_gas_price: input.fair_l2_gas_price,
                fair_pubdata_price: input.l1_gas_price * L1_GAS_PER_PUBDATA_BYTE as u64,
                l1_gas_price: input.l1_gas_price,
            },
        }
    }

    pub fn for_protocol_version(
        protocol_version: ProtocolVersionId,
        fair_l2_gas_price: u64,
        fair_pubdata_price: Option<u64>,
        l1_gas_price: u64,
    ) -> Self {
        if protocol_version.is_post_1_4_1() {
            Self::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
                fair_l2_gas_price,
                fair_pubdata_price: fair_pubdata_price
                    .expect("Pubdata price must be provided for protocol version 1.4.1"),
                l1_gas_price,
            })
        } else {
            Self::L1Pegged(L1PeggedBatchFeeModelInput {
                fair_l2_gas_price,
                l1_gas_price,
            })
        }
    }

    pub fn stricter(self, other: BatchFeeInput) -> Self {
        match (self, other) {
            (BatchFeeInput::L1Pegged(first), BatchFeeInput::L1Pegged(second)) => Self::l1_pegged(
                first.l1_gas_price.max(second.l1_gas_price),
                first.fair_l2_gas_price.max(second.fair_l2_gas_price),
            ),
            input @ (_, _) => {
                let (first, second) = (
                    input.0.into_pubdata_independent(),
                    input.1.into_pubdata_independent(),
                );

                Self::pubdata_independent(
                    first.l1_gas_price.max(second.l1_gas_price),
                    first.fair_l2_gas_price.max(second.fair_l2_gas_price),
                    first.fair_pubdata_price.max(second.fair_pubdata_price),
                )
            }
        }
    }
}

/// Pubdata is only published via calldata and so its price is pegged to the L1 gas price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct L1PeggedBatchFeeModelInput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// The L1 gas price to provide to the VM.
    pub l1_gas_price: u64,
}

impl L1PeggedBatchFeeModelInput {
    pub fn scale_fair_l2_gas_price(&self, scale_factor: f64) -> L1PeggedBatchFeeModelInput {
        L1PeggedBatchFeeModelInput {
            l1_gas_price: self.l1_gas_price,
            fair_l2_gas_price: (self.fair_l2_gas_price as f64 * scale_factor) as u64,
        }
    }
}

/// Pubdata price may be independent from L1 gas price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PubdataIndependentBatchFeeModelInput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    pub fair_pubdata_price: u64,
    /// The L1 gas price to provide to the VM. Even if some of the VM versions may not use this value, it is still maintained for backward compatibility.
    pub l1_gas_price: u64,
}

impl PubdataIndependentBatchFeeModelInput {
    /// Scales the fair L2 gas price. This method shouldn't be used anywhere outside API.
    pub fn scale_fair_l2_gas_price(
        self,
        scale_factor: f64,
    ) -> PubdataIndependentBatchFeeModelInput {
        clip_batch_fee_model_input_v2(PubdataIndependentBatchFeeModelInput {
            fair_l2_gas_price: (self.fair_l2_gas_price as f64 * scale_factor) as u64,
            fair_pubdata_price: self.fair_pubdata_price,
            l1_gas_price: self.l1_gas_price,
        })
    }
}

/// The enum which represents the version of the fee model. It is used to determine which fee model should be used for the batch.
/// - `V1`, the first model that was used in ZKsync Era. In this fee model, the pubdata price must be pegged to the L1 gas price.
///   Also, the fair L2 gas price is expected to only include the proving/computation price for the operator and not the costs that come from
///   processing the batch on L1.
/// - `V2`, the second model that was used in ZKsync Era. There the pubdata price might be independent from the L1 gas price. Also,
///   The fair L2 gas price is expected to both the proving/computation price for the operator and the costs that come from
///   processing the batch on L1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FeeModelConfig {
    V1(FeeModelConfigV1),
    V2(FeeModelConfigV2),
}

/// Config params for the first version of the fee model.
///
/// Here, the pubdata price is pegged to the L1 gas price and neither fair L2 gas price
/// nor the pubdata price include the overhead for closing the batch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeModelConfigV1 {
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    /// Unlike the `V2`, this price will be directly used as the `fair_l2_gas_price` in the bootloader.
    pub minimal_l2_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeModelConfigV2 {
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    pub minimal_l2_gas_price: u64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of computation resources.
    /// It has range from 0 to 1. If it is 0, the compute will not depend on the cost for closing the batch.
    /// If it is 1, the gas limit per batch will have to cover the entire cost of closing the batch.
    pub compute_overhead_part: f64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
    /// It has range from 0 to 1. If it is 0, the pubdata will not depend on the cost for closing the batch.
    /// If it is 1, the pubdata limit per batch will have to cover the entire cost of closing the batch.
    pub pubdata_overhead_part: f64,
    /// The constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
    pub batch_overhead_l1_gas: u64,
    /// The maximum amount of gas that can be used by the batch. This value is derived from the circuits limitation per batch.
    pub max_gas_per_batch: u64,
    /// The maximum amount of pubdata that can be used by the batch. Note that if the calldata is used as pubdata, this variable should not exceed 128kb.
    pub max_pubdata_per_batch: u64,
}

impl Default for FeeModelConfig {
    /// Config with all zeroes is not a valid config (since for instance having 0 max gas per batch may incur division by zero),
    /// so we implement a sensible default config here.
    fn default() -> Self {
        Self::V1(FeeModelConfigV1 {
            minimal_l2_gas_price: 100_000_000,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeParamsV1 {
    pub config: FeeModelConfigV1,
    pub l1_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeParamsV2 {
    config: FeeModelConfigV2,
    l1_gas_price: u64,
    l1_pubdata_price: u64,
    conversion_ratio: BaseTokenConversionRatio,
}

impl FeeParamsV2 {
    pub fn new(
        config: FeeModelConfigV2,
        l1_gas_price: u64,
        l1_pubdata_price: u64,
        conversion_ratio: BaseTokenConversionRatio,
    ) -> Self {
        Self {
            config,
            l1_gas_price,
            l1_pubdata_price,
            conversion_ratio,
        }
    }

    /// Returns the fee model config with the minimal L2 gas price denominated in the chain's base token (WEI or equivalent).
    pub fn config(&self) -> FeeModelConfigV2 {
        FeeModelConfigV2 {
            minimal_l2_gas_price: self.convert_to_base_token(self.config.minimal_l2_gas_price),
            ..self.config
        }
    }

    /// Returns the l1 gas price denominated in the chain's base token (WEI or equivalent).
    pub fn l1_gas_price(&self) -> u64 {
        self.convert_to_base_token(self.l1_gas_price)
    }

    /// Returns the l1 pubdata price denominated in the chain's base token (WEI or equivalent).
    pub fn l1_pubdata_price(&self) -> u64 {
        self.convert_to_base_token(self.l1_pubdata_price)
    }

    /// Converts the fee param to the base token.
    fn convert_to_base_token(&self, price_in_wei: u64) -> u64 {
        let converted_price = u128::from(price_in_wei)
            * u128::from(self.conversion_ratio.numerator.get())
            / u128::from(self.conversion_ratio.denominator.get());

        // Match on the converted price to ensure it can be represented as a u64
        match converted_price.try_into() {
            Ok(converted_price) => converted_price,
            Err(_) => {
                if converted_price > u128::from(u64::MAX) {
                    tracing::warn!(
                        "Conversion to base token price failed: converted price is too large: {}. Using u64::MAX instead.",
                        converted_price
                    );
                } else {
                    panic!(
                        "Conversion to base token price failed: converted price is not a valid u64: {}",
                        converted_price
                    );
                }
                u64::MAX
            }
        }
    }
}

/// The struct that represents the BaseToken<->ETH conversion ratio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BaseTokenConversionRatio {
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
}

impl Default for BaseTokenConversionRatio {
    fn default() -> Self {
        Self {
            numerator: NonZeroU64::new(1).unwrap(),
            denominator: NonZeroU64::new(1).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FeeParams {
    V1(FeeParamsV1),
    V2(FeeParamsV2),
}

impl FeeParams {
    // Sometimes for temporary usage or tests a "sensible" default, i.e. the one consisting of non-zero values is needed.
    pub fn sensible_v1_default() -> Self {
        Self::V1(FeeParamsV1 {
            config: FeeModelConfigV1 {
                minimal_l2_gas_price: 100_000_000,
            },
            l1_gas_price: 1_000_000_000,
        })
    }

    /// Provides scaled [`BatchFeeInput`] based on these parameters.
    pub fn scale(
        self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> BatchFeeInput {
        match self {
            Self::V1(params) => BatchFeeInput::L1Pegged(compute_batch_fee_model_input_v1(
                params,
                l1_gas_price_scale_factor,
            )),
            Self::V2(params) => BatchFeeInput::PubdataIndependent(clip_batch_fee_model_input_v2(
                compute_batch_fee_model_input_v2(
                    params,
                    l1_gas_price_scale_factor,
                    l1_pubdata_price_scale_factor,
                ),
            )),
        }
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_fee_params_v2_safely_hit_u64_max() {
        // In this test we check that hitting borderline scenarios for u64::MAX works as expected
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
            u64::MAX,
            u64::MAX - 1,
            BaseTokenConversionRatio {
                numerator: NonZeroU64::new(u64::MAX).unwrap(),
                denominator: NonZeroU64::new(u64::MAX).unwrap(),
            },
        );
        assert_eq!(params.l1_gas_price(), u64::MAX);
        assert_eq!(params.l1_pubdata_price(), u64::MAX - 1);
    }
}

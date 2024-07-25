use std::num::NonZeroU64;

use bigdecimal::{BigDecimal, ToPrimitive};
use serde::{Deserialize, Serialize};
use zksync_config::configs::chain::{FeeModelVersion, StateKeeperConfig};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

use crate::ProtocolVersionId;

/// Fee input to be provided into the VM. It contains two options:
/// - `L1Pegged`: L1 gas price is provided to the VM, and the pubdata price is derived from it. Using this option is required for the
/// versions of Era prior to 1.4.1 integration.
/// - `PubdataIndependent`: L1 gas price and pubdata price are not necessarily dependent on one another. This options is more suitable for the
/// versions of Era after the 1.4.1 integration. It is expected that if a VM supports `PubdataIndependent` version, then it should also support `L1Pegged` version, but converting it into `PubdataIndependentBatchFeeModelInput` in-place.
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

/// The enum which represents the version of the fee model. It is used to determine which fee model should be used for the batch.
/// - `V1`, the first model that was used in ZKsync Era. In this fee model, the pubdata price must be pegged to the L1 gas price.
/// Also, the fair L2 gas price is expected to only include the proving/computation price for the operator and not the costs that come from
/// processing the batch on L1.
/// - `V2`, the second model that was used in ZKsync Era. There the pubdata price might be independent from the L1 gas price. Also,
/// The fair L2 gas price is expected to both the proving/computation price for the operator and the costs that come from
/// processing the batch on L1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FeeModelConfig {
    V1(FeeModelConfigV1),
    V2(FeeModelConfigV2),
}

/// Config params for the first version of the fee model. Here, the pubdata price is pegged to the L1 gas price and
/// neither fair L2 gas price nor the pubdata price include the overhead for closing the batch
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

impl FeeModelConfig {
    pub fn from_state_keeper_config(state_keeper_config: &StateKeeperConfig) -> Self {
        match state_keeper_config.fee_model_version {
            FeeModelVersion::V1 => Self::V1(FeeModelConfigV1 {
                minimal_l2_gas_price: state_keeper_config.minimal_l2_gas_price,
            }),
            FeeModelVersion::V2 => Self::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: state_keeper_config.minimal_l2_gas_price,
                compute_overhead_part: state_keeper_config.compute_overhead_part,
                pubdata_overhead_part: state_keeper_config.pubdata_overhead_part,
                batch_overhead_l1_gas: state_keeper_config.batch_overhead_l1_gas,
                max_gas_per_batch: state_keeper_config.max_gas_per_batch,
                max_pubdata_per_batch: state_keeper_config.max_pubdata_per_batch,
            }),
        }
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
        let conversion_ratio = BigDecimal::from(self.conversion_ratio.numerator.get())
            / BigDecimal::from(self.conversion_ratio.denominator.get());
        let converted_price_bd = BigDecimal::from(price_in_wei) * conversion_ratio;

        // Match on the converted price to ensure it can be represented as a u64
        match converted_price_bd.to_u64() {
            Some(converted_price) => converted_price,
            None => {
                if converted_price_bd > BigDecimal::from(u64::MAX) {
                    tracing::warn!(
                        "Conversion to base token price failed: converted price is too large: {}. Using u64::MAX instead.",
                        converted_price_bd
                    );
                } else {
                    panic!(
                        "Conversion to base token price failed: converted price is not a valid u64: {}",
                        converted_price_bd
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
}

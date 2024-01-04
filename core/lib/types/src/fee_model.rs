use serde::{Deserialize, Serialize};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

/// Fee input to be provided into the VM. It contains two options:
/// - L1Pegged: L1 gas price is provided to the VM, and the pubdata price is derived from it. Using this option is required for the
/// older versions of Era.
/// - PubdataIndependent: L1 gas price and pubdata price are not necessarily dependend on one another. This options is more suitable for the
/// newer versions of Era. It is expected that if a VM supports `PubdataIndependent` version, then it should also support `L1Pegged` version, but converting it into PubdataIndependentBatchFeeModelInput in-place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchFeeInput {
    L1Pegged(L1PeggedBatchFeeModelInput),
    PubdataIndependent(PubdataIndependentBatchFeeModelInput),
}

impl BatchFeeInput {
    // Sometimes for temporary usage or tests a "sensible" default, i.e. the one consisting of non-zero values is needed.
    pub fn sensible_v1_default() -> Self {
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
}

/// Pubdata is only published via calldata and so its price is pegged to the L1 gas price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1PeggedBatchFeeModelInput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// The L1 gas price to provide to the VM.
    pub l1_gas_price: u64,
}

/// Pubdata price may be independent from L1 gas price. The L1 gas price is not needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PubdataIndependentBatchFeeModelInput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    pub fair_pubdata_price: u64,
    /// The L1 gas price to provide to the VM. Even if some of the VM versions may not use this value, it is still maintained for backward compatibility.
    pub l1_gas_price: u64,
}

/// The enum which represents the version of the fee model. It is used to determine which fee model should be used for the batch.
/// - V1, the first model that was used in the zkSync. In this fee model, the pubdata price must be pegged to the L1 gas price.
/// Also, the fair L2 gas price is expected to only include the proving/computation price for the operator and not the overhead.
/// - V2, the second model that was used in the zkSync. There the pubdata price might be independent from the L1 gas price. Also,
/// The fair L2 gas price is expected to both the proving/computation price for the operator and the overhead for closing the batch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MainNodeFeeModelConfig {
    V1(MainNodeFeeModelConfigV1),
    V2(MainNodeFeeModelConfigV2),
}

/// Config params for the first version of the fee model. Here, the pubdata price is pegged to the L1 gas price and
/// neither fair L2 gas price nor the pubdata price include the overhead for closing the batch
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeModelConfigV1 {
    /// The factor by which the L1 gas price is scaled. This is used to account for the fact that the L1 gas price may fluctuate.
    pub l1_gas_price_scale_factor: f64,
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    pub minimal_l2_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeModelConfigV2 {
    /// The factor by which the L1 gas price is scaled. This is used to account for the fact that the L1 gas price may fluctuate.
    pub l1_gas_price_scale_factor: f64,
    /// The factor by which the L1 pubdata price is scaled. This is used to account for the fact that the L1 pubdata price may fluctuate.
    pub l1_pubdata_price_scale_factor: f64,
    /// The minimal acceptable L2 gas price, i.e. the price that should include the cost of computation/proving as well
    /// as potentially premium for congestion.
    pub minimal_l2_gas_price: u64,
    /// Tthe constant that represents the possibility that a batch can be sealed because of overuse of compute.
    pub compute_overhead_percent: f64,
    /// The constant that represents the possibility that a batch can be sealed because of overuse of pubdata.
    pub pubdata_overhead_percent: f64,
    /// The constant amount of L1 gas that is used as the overhead for the batch. It includes the price for batch verification, etc.
    pub batch_overhead_l1_gas: u64,
    /// The maximum amount of gas that can be used by the batch.
    pub max_gas_per_batch: u64,
    /// The maximum amount of pubdata that can be used by the batch.
    pub max_pubdata_per_batch: u64,
}

impl Default for MainNodeFeeModelConfig {
    /// Config with all 0s is not a valid config (since for instance having 0 max gas per batch may incur division by zero),
    /// so we implement a sensible default config here.
    fn default() -> Self {
        Self::V1(MainNodeFeeModelConfigV1 {
            l1_gas_price_scale_factor: 1.0,
            minimal_l2_gas_price: 100_000_000,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeParamsV1 {
    pub config: MainNodeFeeModelConfigV1,
    pub l1_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeParamsV2 {
    pub config: MainNodeFeeModelConfigV2,
    pub l1_gas_price: u64,
    pub l1_pubdata_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MainNodeFeeParams {
    V1(MainNodeFeeParamsV1),
    V2(MainNodeFeeParamsV2),
}

impl MainNodeFeeParams {
    // Sometimes for temporary usage or tests a "sensible" default, i.e. the one consisting of non-zero values is needed.
    pub fn sensible_v1_default() -> Self {
        Self::V1(MainNodeFeeParamsV1 {
            config: MainNodeFeeModelConfigV1 {
                l1_gas_price_scale_factor: 1.0,
                minimal_l2_gas_price: 100_000_000,
            },
            l1_gas_price: 1_000_000_000,
        })
    }
}

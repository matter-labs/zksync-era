use core::panic;

use serde::{Deserialize, Serialize};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

use crate::ProtocolVersionId;

/// Fee input to be provided into the VM. It contains two options:
/// - L1Pegged: L1 gas price is provided to the VM, and the pubdata price is derived from it. Using this option is required for the
/// older versions of Era.
/// - PubdataIndependent: L1 gas price and pubdata price are not necessarily dependend on one another. This options is more suitable for the
/// newer versions of Era.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchFeeInput {
    L1Pegged(L1PeggedBatchFeeModelInput),
    PubdataIndependent(PubdataIndependentBatchFeeModelInput),
}

impl BatchFeeInput {
    pub fn zero() -> Self {
        Self::L1Pegged(L1PeggedBatchFeeModelInput {
            l1_gas_price: 0,
            fair_l2_gas_price: 0,
        })
    }
}

impl Default for BatchFeeInput {
    fn default() -> Self {
        // We have a sensible default value of 1 gwei for L1 gas price and 0.1 gwei for fair L2 gas price.
        Self::L1Pegged(L1PeggedBatchFeeModelInput {
            l1_gas_price: 1_000_000_000,
            fair_l2_gas_price: 100_000_000,
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

    pub fn l1_pegged_ref_mut(&mut self) -> &mut L1PeggedBatchFeeModelInput {
        match self {
            BatchFeeInput::L1Pegged(input) => input,
            _ => panic!("Can not convert PubdataIndependentBatchFeeModelInput into L1PeggedBatchFeeModelInput"),
        }
    }

    pub fn l1_pegged_ref(&self) -> &L1PeggedBatchFeeModelInput {
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
                fair_pubdata_price: input.l1_gas_price * 17,
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
        if protocol_version.is_1_4_1() {
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
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub fair_pubdata_price: u64,
    /// The L1 gas price to provide to the VM. Even if some of the VM versions may not use this value, it is still maintained for backward compatibility.
    pub l1_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeModelConfig {
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
        Self {
            // We don't scale L1 prices by default
            l1_gas_price_scale_factor: 1.0,
            l1_pubdata_price_scale_factor: 1.0,
            minimal_l2_gas_price: 100_000_000, // 0.1 gwei
            compute_overhead_percent: 0.0,
            pubdata_overhead_percent: 1.0, // We assume that all the batches are closed because of pubdata limit
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 250_000_000,
            max_pubdata_per_batch: 120_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeParams {
    pub config: MainNodeFeeModelConfig,
    pub l1_gas_price: u64,
    pub l1_pubdata_price: u64,
}

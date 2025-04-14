use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AggregatedActionType {
    Commit,
    PublishProofOnchain,
    Execute,
}

impl AggregatedActionType {
    pub fn as_str(self) -> &'static str {
        // "Blocks" suffixes are there for legacy reasons
        match self {
            Self::Commit => "CommitBlocks",
            Self::PublishProofOnchain => "PublishProofBlocksOnchain",
            Self::Execute => "ExecuteBlocks",
        }
    }
}

impl fmt::Display for AggregatedActionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AggregatedActionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CommitBlocks" => Ok(Self::Commit),
            "PublishProofBlocksOnchain" => Ok(Self::PublishProofOnchain),
            "ExecuteBlocks" => Ok(Self::Execute),
            _ => Err(
                "Incorrect aggregated action type; expected one of `CommitBlocks`, `PublishProofBlocksOnchain`, \
                `ExecuteBlocks`",
            ),
        }
    }
}

#[derive(Debug)]
pub struct GasConsts;

#[derive(Debug)]
pub struct CommitGasConsts {
    pub base: u32,
    pub per_batch: u32,
}

#[derive(Debug)]
pub struct ExecuteCosts {
    pub base: u32,
    pub per_batch: u32,
    pub per_l1_l2_tx: u32,
}

impl GasConsts {
    /// Base gas cost of processing aggregated `Execute` operation.
    /// It's applicable iff SL is Ethereum.
    const AGGR_L1_BATCH_EXECUTE_BASE_COST: u32 = 200_000;
    /// Base gas cost of processing aggregated `Execute` operation.
    /// It's applicable if SL is  Gateway.
    const AGGR_GATEWAY_BATCH_EXECUTE_BASE_COST: u32 = 300_000;

    /// Base gas cost of processing aggregated `Commit` operation.
    /// It's applicable if SL is Ethereum.
    const AGGR_L1_BATCH_COMMIT_BASE_COST: u32 = 242_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Ethereum.
    const L1_BATCH_COMMIT_BASE_COST: u32 = 31_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Gateway.
    const AGGR_GATEWAY_BATCH_COMMIT_BASE_COST: u32 = 150_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Gateway.
    const GATEWAY_BATCH_COMMIT_BASE_COST: u32 = 200_000;

    /// All gas cost of processing `PROVE` operation per batch.
    /// It's applicable if SL is GATEWAY.
    /// TODO calculate it properly
    const GATEWAY_BATCH_PROOF_GAS_COST: u32 = 1_600_000;

    /// All gas cost of processing `PROVE` operation per batch.
    /// It's applicable if SL is Ethereum.
    const L1_BATCH_PROOF_GAS_COST_ETHEREUM: u32 = 800_000;

    /// Base gas cost of processing `EXECUTION` operation per batch.
    /// It's applicable if SL is GATEWAY.
    const GATEWAY_BATCH_EXECUTION_COST: u32 = 100_000;
    /// Gas cost of processing `l1_operation` in batch.
    /// It's applicable if SL is GATEWAY.
    const GATEWAY_L1_OPERATION_COST: u32 = 4_000;

    /// Additional gas cost of processing `Execute` operation per batch.
    /// It's applicable iff SL is Ethereum.
    const L1_BATCH_EXECUTE_BASE_COST: u32 = 30_000;

    /// Additional gas cost of processing `Execute` operation per L1->L2 tx.
    /// It's applicable iff SL is Ethereum.
    const L1_OPERATION_EXECUTE_COST: u32 = 12_500;

    pub fn commit_costs(is_gateway: bool) -> CommitGasConsts {
        if is_gateway {
            CommitGasConsts {
                base: Self::GATEWAY_BATCH_COMMIT_BASE_COST,
                per_batch: Self::AGGR_GATEWAY_BATCH_COMMIT_BASE_COST,
            }
        } else {
            CommitGasConsts {
                base: Self::L1_BATCH_COMMIT_BASE_COST,
                per_batch: Self::AGGR_L1_BATCH_COMMIT_BASE_COST,
            }
        }
    }

    pub fn proof_costs(is_gateway: bool) -> u32 {
        if is_gateway {
            Self::GATEWAY_BATCH_PROOF_GAS_COST
        } else {
            Self::L1_BATCH_PROOF_GAS_COST_ETHEREUM
        }
    }

    pub fn execute_costs(is_gateway: bool) -> ExecuteCosts {
        if is_gateway {
            ExecuteCosts {
                base: Self::AGGR_GATEWAY_BATCH_EXECUTE_BASE_COST,
                per_batch: Self::GATEWAY_BATCH_EXECUTION_COST,
                per_l1_l2_tx: Self::GATEWAY_L1_OPERATION_COST,
            }
        } else {
            ExecuteCosts {
                base: Self::AGGR_L1_BATCH_EXECUTE_BASE_COST,
                per_batch: Self::L1_BATCH_EXECUTE_BASE_COST,
                per_l1_l2_tx: Self::L1_OPERATION_EXECUTE_COST,
            }
        }
    }
}

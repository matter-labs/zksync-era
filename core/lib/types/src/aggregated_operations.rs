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

/// Additional gas cost of processing `Execute` operation per batch.
/// It's applicable iff SL is Ethereum.
pub const L1_BATCH_EXECUTE_BASE_COST: u32 = 30_000;

/// Additional gas cost of processing `Execute` operation per L1->L2 tx.
/// It's applicable iff SL is Ethereum.
pub const L1_OPERATION_EXECUTE_COST: u32 = 12_500;

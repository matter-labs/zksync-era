use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AggregatedOperationType {
    Commit,
    PublishProofOnChain,
    Execute,
}
impl AggregatedOperationType {
    pub fn action_type(self) -> AggregatedActionType {
        match self {
            AggregatedOperationType::Commit => AggregatedActionType::Commit,
            AggregatedOperationType::PublishProofOnChain => {
                AggregatedActionType::PublishProofOnChain
            }
            AggregatedOperationType::Execute => AggregatedActionType::Execute,
        }
    }
}
impl fmt::Display for AggregatedOperationType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.action_type().as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AggregatedActionType {
    Commit,
    PublishProofOnChain,
    Execute,
    Tee,
}

impl AggregatedActionType {
    pub fn as_str(self) -> &'static str {
        // "Blocks" suffixes are there for legacy reasons
        match self {
            Self::Commit => "CommitBlocks",
            Self::PublishProofOnChain => "PublishProofBlocksOnchain",
            Self::Execute => "ExecuteBlocks",
            Self::Tee => "TEE",
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
            "PublishProofBlocksOnchain" => Ok(Self::PublishProofOnChain),
            "ExecuteBlocks" => Ok(Self::Execute),
            "TEE" => Ok(Self::Tee),
            _ => Err(
                "Incorrect aggregated action type; expected one of `CommitBlocks`, `PublishProofBlocksOnchain`, \
                `ExecuteBlocks`, `TEE`",
            ),
        }
    }
}

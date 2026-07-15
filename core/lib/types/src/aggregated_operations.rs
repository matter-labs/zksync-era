use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum L2BlockAggregatedActionType {
    Precommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum L1BatchAggregatedActionType {
    Commit,
    PublishProofOnchain,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AggregatedActionType {
    L2Block(L2BlockAggregatedActionType),
    L1Batch(L1BatchAggregatedActionType),
}

impl L2BlockAggregatedActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Precommit => "Precommit",
        }
    }
}

impl L1BatchAggregatedActionType {
    pub fn as_str(self) -> &'static str {
        // "Blocks" suffixes are there for legacy reasons
        match self {
            Self::Commit => "CommitBlocks",
            Self::PublishProofOnchain => "PublishProofBlocksOnchain",
            Self::Execute => "ExecuteBlocks",
        }
    }
}

impl fmt::Display for L1BatchAggregatedActionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for L1BatchAggregatedActionType {
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

impl fmt::Display for L2BlockAggregatedActionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for L2BlockAggregatedActionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Precommit" => Ok(Self::Precommit),
            _ => Err(
                "Incorrect aggregated action type; expected one of `CommitBlocks`, `PublishProofBlocksOnchain`, \
                `ExecuteBlocks`",
            ),
        }
    }
}

impl AggregatedActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::L2Block(action) => action.as_str(),
            Self::L1Batch(action) => action.as_str(),
        }
    }
}
impl FromStr for AggregatedActionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(action) = L2BlockAggregatedActionType::from_str(s) {
            return Ok(Self::L2Block(action));
        }
        if let Ok(action) = L1BatchAggregatedActionType::from_str(s) {
            return Ok(Self::L1Batch(action));
        }
        Err("Incorrect aggregated action type")
    }
}

impl fmt::Display for AggregatedActionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<L2BlockAggregatedActionType> for AggregatedActionType {
    fn from(action: L2BlockAggregatedActionType) -> Self {
        Self::L2Block(action)
    }
}

impl From<L1BatchAggregatedActionType> for AggregatedActionType {
    fn from(action: L1BatchAggregatedActionType) -> Self {
        Self::L1Batch(action)
    }
}

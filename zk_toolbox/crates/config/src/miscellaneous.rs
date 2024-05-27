use std::{fmt::Display, str::FromStr};

use alloy_primitives::Address;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
    Default,
    PartialEq,
    Eq,
)]
pub enum L1BatchCommitDataGeneratorMode {
    #[default]
    Rollup,
    Validium,
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
    PartialEq,
    Eq,
)]
pub enum ProverMode {
    NoProofs,
    Gpu,
    Cpu,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChainId(pub u32);

impl Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for ChainId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
)]
pub enum L1Network {
    #[default]
    Localhost,
    Sepolia,
    Mainnet,
}

impl L1Network {
    pub fn chain_id(&self) -> u32 {
        match self {
            L1Network::Localhost => 9,
            L1Network::Sepolia => 11155111,
            L1Network::Mainnet => 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct BaseToken {
    pub address: Address,
    pub nominator: u64,
    pub denominator: u64,
}

impl BaseToken {
    pub fn eth() -> Self {
        Self {
            nominator: 1,
            denominator: 1,
            address: Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
)]
pub enum WalletCreation {
    /// Load wallets from localhost mnemonic, they are funded for localhost env
    #[default]
    Localhost,
    /// Generate random wallets
    Random,
    /// Generate placeholder wallets
    Empty,
    /// Specify file with wallets
    InFile,
}

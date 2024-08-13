use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

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
    strum::Display,
)]
pub enum L1Network {
    #[default]
    Localhost,
    Sepolia,
    Mainnet,
}

impl L1Network {
    #[must_use]
    pub fn chain_id(&self) -> u64 {
        match self {
            L1Network::Localhost => 9,
            L1Network::Sepolia => 11_155_111,
            L1Network::Mainnet => 1,
        }
    }
}

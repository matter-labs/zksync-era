use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

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
    #[must_use]
    pub fn chain_id(&self) -> u32 {
        match self {
            L1Network::Localhost => 9,
            L1Network::Sepolia => 11_155_111,
            L1Network::Mainnet => 1,
        }
    }
}

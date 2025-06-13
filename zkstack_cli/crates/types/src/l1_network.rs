use std::str::FromStr;

use clap::ValueEnum;
use ethers::types::Address;
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
    Sepolia,
    Localhost,
    Holesky,
    Mainnet,
}

impl L1Network {
    #[must_use]
    pub fn chain_id(&self) -> u64 {
        match self {
            L1Network::Sepolia => 11_155_111,
            L1Network::Localhost => 9,
            L1Network::Holesky => 17000,
            L1Network::Mainnet => 1,
        }
    }

    pub fn avail_l1_da_validator_addr(&self) -> Option<Address> {
        match self {
            L1Network::Localhost => None,
            L1Network::Sepolia | L1Network::Holesky => {
                Some(Address::from_str("0x73d59fe232fce421d1365d6a5beec49acde3d0d9").unwrap())
            }
            L1Network::Mainnet => None, // TODO: add mainnet address after it is known
        }
    }
}

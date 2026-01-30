use std::str::FromStr;

use clap::ValueEnum;
use ethers::types::{Address, H256};
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
    Holesky,
    Mainnet,
}

impl L1Network {
    #[must_use]
    pub fn chain_id(&self) -> u64 {
        match self {
            L1Network::Localhost => 9,
            L1Network::Sepolia => 11_155_111,
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

    pub fn zk_token_asset_id(&self) -> H256 {
        match self {
            L1Network::Localhost => {
                // When testing locally, we deploy the ZK token after ecosystem init, so we need to derive its asset id
                // The address where ZK will be deployed at is 0x57dba6a9b498265c4414b129a3b2ad6e80fca518
                H256::from_str("0x1da996953cdc5fb80dd9e37853dc8e02efb528dc0c7ca62965f6fb2afe867792")
                    .unwrap()
            }
            L1Network::Sepolia => {
                // https://sepolia.etherscan.io/address/0x2569600E58850a0AaD61F7Dd2569516C3d909521#readProxyContract#F3
                H256::from_str("0x0d643837c76916220dfe0d5e971cfc3dc2c7569b3ce12851c8e8f17646d86bca")
                    .unwrap()
            }
            L1Network::Mainnet => {
                // https://etherscan.io/address/0x66A5cFB2e9c529f14FE6364Ad1075dF3a649C0A5#readProxyContract#F3
                H256::from_str("0x83e2fbc0a739b3c765de4c2b4bf8072a71ea8fbb09c8cf579c71425d8bc8804a")
                    .unwrap()
            }
            L1Network::Holesky => H256::zero(),
        }
    }
}

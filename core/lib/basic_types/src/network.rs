//! The network where the ZKsync resides.
//!

// Built-in uses
use std::{fmt, str::FromStr};

// External uses
use serde::{Deserialize, Serialize};

// Workspace uses
use crate::L1ChainId;

// Local uses

/// Network to be used for a ZKsync client.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Network {
    /// Ethereum Mainnet.
    Mainnet,
    /// Ethereum Rinkeby testnet.
    Rinkeby,
    /// Ethereum Ropsten testnet.
    Ropsten,
    /// Ethereum Görli testnet.
    Goerli,
    /// Ethereum Sepolia testnet.
    Sepolia,
    /// Self-hosted Ethereum network.
    Localhost,
    /// Default L2 development network. Note, that unlike other networks, it
    /// is actually an Era-based L2, not an L1.
    LocalhostL2,
    /// Unknown network type.
    Unknown,
    /// Test network for testkit purposes
    Test,
}

impl FromStr for Network {
    type Err = String;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        Ok(match string {
            "mainnet" => Self::Mainnet,
            "rinkeby" => Self::Rinkeby,
            "ropsten" => Self::Ropsten,
            "goerli" => Self::Goerli,
            "localhost" => Self::Localhost,
            "localhostL2" => Self::LocalhostL2,
            "sepolia" => Self::Sepolia,
            "test" => Self::Test,
            another => return Err(another.to_owned()),
        })
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Rinkeby => write!(f, "rinkeby"),
            Self::Ropsten => write!(f, "ropsten"),
            Self::Goerli => write!(f, "goerli"),
            Self::Localhost => write!(f, "localhost"),
            Self::LocalhostL2 => write!(f, "LocalhostL2"),
            Self::Sepolia => write!(f, "sepolia"),
            Self::Unknown => write!(f, "unknown"),
            Self::Test => write!(f, "test"),
        }
    }
}

impl Network {
    /// Returns the network chain ID on the Ethereum side.
    pub fn from_chain_id(chain_id: L1ChainId) -> Self {
        match *chain_id {
            1 => Self::Mainnet,
            3 => Self::Ropsten,
            4 => Self::Rinkeby,
            5 => Self::Goerli,
            9 => Self::Localhost,
            270 => Self::LocalhostL2,
            11155111 => Self::Sepolia,
            _ => Self::Unknown,
        }
    }

    /// Returns the network chain ID on the Ethereum side.
    pub fn chain_id(self) -> L1ChainId {
        match self {
            Self::Mainnet => L1ChainId(1),
            Self::Ropsten => L1ChainId(3),
            Self::Rinkeby => L1ChainId(4),
            Self::Goerli => L1ChainId(5),
            Self::Localhost => L1ChainId(9),
            Self::LocalhostL2 => L1ChainId(270),
            Self::Sepolia => L1ChainId(11155111),
            Self::Unknown => panic!("Unknown chain ID"),
            Self::Test => panic!("Test chain ID"),
        }
    }
}

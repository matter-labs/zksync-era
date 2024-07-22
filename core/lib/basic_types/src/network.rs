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
    /// Unknown network type.
    Unknown,
    /// Test network for testkit purposes
    Test,
    /// Linea testnet
    Lineatest,
    /// Linea mainnet
    Linea,
    /// Arbitrum Sepolia testnet
    Arbitrumtest,
    /// Arbitrum mainnet
    Arbitrum,
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
            "sepolia" => Self::Sepolia,
            "test" => Self::Test,
            "lineatest" => Self::Lineatest,
            "linea" => Self::Linea,
            "arbitrumtest" => Self::Arbitrumtest,
            "arbitrum" => Self::Arbitrum,
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
            Self::Sepolia => write!(f, "sepolia"),
            Self::Unknown => write!(f, "unknown"),
            Self::Test => write!(f, "test"),
            Self::Lineatest => write!(f, "lineatest"),
            Self::Linea => write!(f, "linea"),
            Self::Arbitrumtest => write!(f, "arbitrumtest"),
            Self::Arbitrum => write!(f, "arbitrum"),
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
            11155111 => Self::Sepolia,
            59141 => Self::Lineatest,
            59144 => Self::Linea,
            421614 => Self::Arbitrumtest,
            42161 => Self::Arbitrum,
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
            Self::Sepolia => L1ChainId(11155111),
            Self::Lineatest => L1ChainId(59141),
            Self::Linea => L1ChainId(59144),
            Self::Arbitrumtest => L1ChainId(421614),
            Self::Arbitrum => L1ChainId(42161),
            Self::Unknown => panic!("Unknown chain ID"),
            Self::Test => panic!("Test chain ID"),
        }
    }
}

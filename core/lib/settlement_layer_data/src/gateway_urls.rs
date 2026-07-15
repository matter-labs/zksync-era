use std::str::FromStr;

use once_cell::sync::Lazy;
use zksync_basic_types::{url::SensitiveUrl, Address};

/// Gateway Bridgehub Addresses
pub static MAINNET_BRIDGEHUB_ADDR: Lazy<Address> =
    Lazy::new(|| Address::from_str("0x303a465B659cBB0ab36eE643eA362c509EEb5213").unwrap());

pub static STAGE_BRIDGEHUB_ADDR: Lazy<Address> =
    Lazy::new(|| Address::from_str("0x236D1c3Ff32Bd0Ca26b72Af287E895627c0478cE").unwrap());

pub static TESTNET_BRIDGEHUB_ADDR: Lazy<Address> =
    Lazy::new(|| Address::from_str("0x35A54c8C757806eB6820629bc82d90E056394C92").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultGatewayUrl {
    Testnet,
    Mainnet,
    Stage,
}

/// While MatterLabs is the only gateway provider, it's safe to use default parameters for the gateway client.
/// These URLs serve as fallbacks in case the gateway URL is not specified in the secrets.
/// This is not defined in JSON, as it's easier to configure the value in secrets
/// rather than modifying a JSON file.
impl DefaultGatewayUrl {
    pub fn from_bridgehub_address(address: Address) -> Option<Self> {
        match address {
            addr if addr == *TESTNET_BRIDGEHUB_ADDR => Some(Self::Testnet),
            addr if addr == *STAGE_BRIDGEHUB_ADDR => Some(Self::Stage),
            addr if addr == *MAINNET_BRIDGEHUB_ADDR => Some(Self::Mainnet),
            _ => None,
        }
    }

    pub fn to_gateway_url(self) -> SensitiveUrl {
        let url = match self {
            Self::Testnet => "https://rpc.era-gateway-testnet.zksync.dev/",
            Self::Mainnet => "https://rpc.era-gateway-mainnet.zksync.dev/",
            Self::Stage => "https://rpc.era-gateway-stage.zksync.dev/",
        };
        SensitiveUrl::from_str(url).expect("URL is valid")
    }
}

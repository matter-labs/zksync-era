use std::str::FromStr;

use zksync_basic_types::{url::SensitiveUrl, Address};

/// While MatterLabs is the only gateway provider,
/// it's safe to have default params for gateway client.
/// Meanwhile these urls,
/// is a fallback if the gateway url is not presented in secrets
/// It's not a JSON, because it's easier to set the proper value in config
/// instead of fixing the JSON.

const MAINNET_BRIDGEHUB_ADDRESS: once_cell::sync::Lazy<Address> =
    once_cell::sync::Lazy::new(|| {
        Address::from_str("0x303a465B659cBB0ab36eE643eA362c509EEb5213").unwrap()
    });

const STAGE_BRIDGEHUB_ADDRESS: once_cell::sync::Lazy<Address> = once_cell::sync::Lazy::new(|| {
    Address::from_str("0x236D1c3Ff32Bd0Ca26b72Af287E895627c0478cE").unwrap()
});

const TESTNET_BRIDGEHUB_ADDRESS: once_cell::sync::Lazy<Address> =
    once_cell::sync::Lazy::new(|| {
        Address::from_str("0x35A54c8C757806eB6820629bc82d90E056394C92").unwrap()
    });

pub enum DefaultGatewayUrl {
    Testnet,
    Mainnet,
    Stage,
}

impl DefaultGatewayUrl {
    pub fn from_bridgehub_address(address: Address) -> Option<Self> {
        if address == *STAGE_BRIDGEHUB_ADDRESS {
            Some(Self::Stage)
        } else if address == *TESTNET_BRIDGEHUB_ADDRESS {
            Some(Self::Testnet)
        } else if address == *MAINNET_BRIDGEHUB_ADDRESS {
            Some(Self::Mainnet)
        } else {
            None
        }
    }

    pub fn to_gateway_url(self) -> SensitiveUrl {
        match self {
            DefaultGatewayUrl::Testnet => {
                SensitiveUrl::from_str("https://rpc.era-gateway-testnet.zksync.dev/")
                    .expect("Valid URL")
            }
            DefaultGatewayUrl::Mainnet => {
                SensitiveUrl::from_str("https://rpc.era-gateway-mainnet.zksync.dev/")
                    .expect("Valid URL")
            }
            DefaultGatewayUrl::Stage => {
                SensitiveUrl::from_str("https://rpc.era-gateway-stage.zksync.dev/")
                    .expect("Valid URL")
            }
        }
    }
}

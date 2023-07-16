use std::{collections::HashMap, fs::read_to_string, path::PathBuf, str::FromStr};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use zksync_types::network::Network;
use zksync_types::{
    tokens::{TokenMetadata, ETHEREUM_ADDRESS},
    Address,
};
use zksync_utils::parse_env;

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Clone)]
pub struct MockTokenListFetcher {
    tokens: HashMap<Address, TokenMetadata>,
}

impl MockTokenListFetcher {
    pub fn new(network: Network) -> Self {
        let network = network.to_string();
        let tokens: HashMap<_, _> = get_genesis_token_list(&network)
            .into_iter()
            .map(|item| {
                let addr = Address::from_str(&item.address[2..]).unwrap();
                let metadata = TokenMetadata {
                    name: item.name,
                    symbol: item.symbol,
                    decimals: item.decimals,
                };

                (addr, metadata)
            })
            .chain(std::iter::once((
                ETHEREUM_ADDRESS,
                TokenMetadata {
                    name: "Ethereum".into(),
                    symbol: "ETH".into(),
                    decimals: 18,
                },
            )))
            .collect();

        Self { tokens }
    }
}

#[async_trait]
impl FetcherImpl for MockTokenListFetcher {
    async fn fetch_token_list(&self) -> Result<HashMap<Address, TokenMetadata>, ApiFetchError> {
        Ok(self.tokens.clone())
    }
}

/// Tokens that added when deploying contract
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenGenesisListItem {
    /// Address (prefixed with 0x)
    pub address: String,
    /// Powers of 10 in 1.0 token (18 for default ETH-like tokens)
    pub decimals: u8,
    /// Token symbol
    pub symbol: String,
    /// Token name
    pub name: String,
}

fn get_genesis_token_list(network: &str) -> Vec<TokenGenesisListItem> {
    let mut file_path = parse_env::<PathBuf>("ZKSYNC_HOME");
    file_path.push("etc");
    file_path.push("tokens");
    file_path.push(network);
    file_path.set_extension("json");
    serde_json::from_str(&read_to_string(file_path).unwrap()).unwrap()
}

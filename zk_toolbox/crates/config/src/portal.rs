use std::path::Path;

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::traits::SaveConfig;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortalRuntimeConfig {
    #[serde(rename = "nodeType")]
    pub node_type: String,
    #[serde(rename = "hyperchainsConfig")]
    pub hyperchain_configs: HyperchainsConfigs,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainsConfigs(pub Vec<HyperchainConfig>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainConfig {
    pub network: NetworkConfig,
    pub tokens: Vec<TokenConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkConfig {
    pub id: u64,      // L2 Network ID
    pub key: String,  // L2 Network key
    pub name: String, // L2 Network name
    #[serde(rename = "rpcUrl")]
    pub rpc_url: String, // L2 RPC URL
    #[serde(rename = "blockExplorerUrl", skip_serializing_if = "Option::is_none")]
    pub block_explorer_url: Option<String>, // L2 Block Explorer URL
    #[serde(rename = "blockExplorerApi", skip_serializing_if = "Option::is_none")]
    pub block_explorer_api: Option<String>, // L2 Block Explorer API
    #[serde(rename = "publicL1NetworkId", skip_serializing_if = "Option::is_none")]
    pub public_l1_network_id: Option<u64>, // Ethereum Mainnet or Ethereum Sepolia Testnet ID
    #[serde(rename = "l1Network", skip_serializing_if = "Option::is_none")]
    pub l1_network: Option<L1NetworkConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct L1NetworkConfig {
    pub id: u64,
    pub name: String,
    pub network: String,
    #[serde(rename = "nativeCurrency")]
    pub native_currency: NativeCurrency,
    #[serde(rename = "rpcUrls")]
    pub rpc_urls: RpcUrls,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NativeCurrency {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcUrls {
    pub default: RpcUrlConfig,
    pub public: RpcUrlConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcUrlConfig {
    pub http: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenConfig {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(rename = "l1Address", skip_serializing_if = "Option::is_none")]
    pub l1_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl SaveConfig for PortalRuntimeConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        let config_js_content = format!("window['##runtimeConfig'] = {};", json);
        Ok(shell.write_file(path, config_js_content.as_bytes())?)
    }
}

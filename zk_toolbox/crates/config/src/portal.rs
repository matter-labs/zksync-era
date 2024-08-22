use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use types::TokenInfo;
use xshell::Shell;

use crate::{
    consts::{LOCAL_CONFIGS_PATH, PORTAL_CONFIG_FILE},
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PortalRuntimeConfig {
    pub node_type: String,
    pub hyperchains_config: HyperchainsConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainsConfig(pub Vec<HyperchainConfig>);

impl HyperchainsConfig {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainConfig {
    pub network: NetworkConfig,
    pub tokens: Vec<TokenConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub id: u64,         // L2 Network ID
    pub key: String,     // L2 Network key
    pub name: String,    // L2 Network name
    pub rpc_url: String, // L2 RPC URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_explorer_url: Option<String>, // L2 Block Explorer URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_explorer_api: Option<String>, // L2 Block Explorer API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_l1_network_id: Option<u64>, // Ethereum Mainnet or Ethereum Sepolia Testnet ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_network: Option<L1NetworkConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct L1NetworkConfig {
    pub id: u64,
    pub name: String,
    pub network: String,
    pub native_currency: TokenInfo,
    pub rpc_urls: RpcUrls,
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
#[serde(rename_all = "camelCase")]
pub struct TokenConfig {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl PortalRuntimeConfig {
    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(PORTAL_CONFIG_FILE)
    }
}

impl FileConfigWithDefaultName for PortalRuntimeConfig {
    const FILE_NAME: &'static str = PORTAL_CONFIG_FILE;
}

impl SaveConfig for PortalRuntimeConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        // The dapp-portal is served as a pre-built static app in a Docker image.
        // It uses a JavaScript file (config.js) that injects the configuration at runtime
        // by overwriting the '##runtimeConfig' property of the window object.
        // Therefore, we generate a JavaScript file instead of a JSON file.
        // This file will be mounted to the Docker image when it runs.
        let json = serde_json::to_string_pretty(&self)?;
        let config_js_content = format!("window['##runtimeConfig'] = {};", json);
        Ok(shell.write_file(path, config_js_content.as_bytes())?)
    }
}

impl ReadConfig for PortalRuntimeConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config_js_content = shell.read_file(path)?;
        // Extract the JSON part from the JavaScript file
        let json_start = config_js_content
            .find('{')
            .ok_or_else(|| anyhow::anyhow!("Invalid config file format"))?;
        let json_end = config_js_content
            .rfind('}')
            .ok_or_else(|| anyhow::anyhow!("Invalid config file format"))?;
        let json_str = &config_js_content[json_start..=json_end];
        // Parse the JSON into PortalRuntimeConfig
        let config: PortalRuntimeConfig = serde_json::from_str(json_str)?;
        Ok(config)
    }
}

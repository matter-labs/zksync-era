use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_types::TokenInfo;

use crate::{
    consts::{
        LOCAL_APPS_PATH, LOCAL_CONFIGS_PATH, LOCAL_GENERATED_PATH, PORTAL_CONFIG_FILE,
        PORTAL_JS_CONFIG_FILE,
    },
    traits::{FileConfigTrait, ReadConfig, SaveConfig},
};

/// Portal JSON configuration file. This file contains configuration for the portal app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PortalConfig {
    pub node_type: String,
    pub hyperchains_config: Vec<PortalChainConfig>,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortalChainConfig {
    pub network: NetworkConfig,
    pub tokens: Vec<TokenConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub id: u64,         // L2 Network ID
    pub key: String,     // L2 Network key (chain name used during the initialization)
    pub name: String,    // L2 Network name (displayed in the app dropdown)
    pub rpc_url: String, // L2 RPC URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>, // If true, the chain will not be shown in the app dropdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_explorer_url: Option<String>, // L2 Block Explorer URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_explorer_api: Option<String>, // L2 Block Explorer API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_l1_network_id: Option<u64>, // Ethereum Mainnet or Ethereum Sepolia Testnet ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_network: Option<L1NetworkConfig>,
    #[serde(flatten)]
    pub other: serde_json::Value,
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

impl PortalConfig {
    /// Returns the path to the portal configuration file.
    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_APPS_PATH)
            .join(PORTAL_CONFIG_FILE)
    }

    /// Reads the existing config or creates a default one if it doesn't exist.
    pub fn read_or_create_default(shell: &Shell) -> anyhow::Result<Self> {
        let config_path = Self::get_config_path(&shell.current_dir());
        match Self::read(shell, &config_path) {
            Ok(config) => Ok(config),
            Err(_) => {
                let config = Self::default();
                config.save(shell, &config_path)?;
                Ok(config)
            }
        }
    }

    /// Adds or updates a given chain configuration.
    pub fn add_chain_config(&mut self, config: &PortalChainConfig) {
        // Replace if config with the same network key already exists
        if let Some(index) = self
            .hyperchains_config
            .iter()
            .position(|c| c.network.key == config.network.key)
        {
            self.hyperchains_config[index] = config.clone();
            return;
        }
        self.hyperchains_config.push(config.clone());
    }

    /// Retains only the chains whose names are present in the given vector.
    pub fn filter(&mut self, chain_names: &[String]) {
        self.hyperchains_config
            .retain(|config| chain_names.contains(&config.network.key));
    }

    /// Hides all chains except those specified in the given vector.
    pub fn hide_except(&mut self, chain_names: &[String]) {
        for config in &mut self.hyperchains_config {
            config.network.hidden = Some(!chain_names.contains(&config.network.key));
        }
    }

    /// Checks if a chain with the given name exists in the configuration.
    pub fn contains(&self, chain_name: &String) -> bool {
        self.hyperchains_config
            .iter()
            .any(|config| &config.network.key == chain_name)
    }

    pub fn is_empty(&self) -> bool {
        self.hyperchains_config.is_empty()
    }

    pub fn save_as_js(&self, shell: &Shell) -> anyhow::Result<PathBuf> {
        // The dapp-portal is served as a pre-built static app in a Docker image.
        // It uses a JavaScript file (config.js) that injects the configuration at runtime
        // by overwriting the '##runtimeConfig' property of the window object.
        // This file will be mounted to the Docker image when it runs.
        let path = Self::get_generated_js_config_path(&shell.current_dir());
        let json = serde_json::to_string_pretty(&self)?;
        let config_js_content = format!("window['##runtimeConfig'] = {};", json);
        shell.write_file(path.clone(), config_js_content.as_bytes())?;
        Ok(path)
    }

    fn get_generated_js_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_GENERATED_PATH)
            .join(PORTAL_JS_CONFIG_FILE)
    }
}

impl Default for PortalConfig {
    fn default() -> Self {
        PortalConfig {
            node_type: "hyperchain".to_string(),
            hyperchains_config: Vec::new(),
            other: serde_json::Value::Null,
        }
    }
}

impl FileConfigTrait for PortalConfig {}

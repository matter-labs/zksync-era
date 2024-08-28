use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::{
    consts::{
        EXPLORER_CHAIN_CONFIG_FILE, EXPLORER_RUNTIME_CONFIG_FILE, LOCAL_APPS_PATH,
        LOCAL_CHAINS_PATH, LOCAL_CONFIGS_PATH, LOCAL_GENERATED_PATH,
    },
    traits::{ReadConfig, SaveConfig, ZkToolboxConfig},
};

/// Explorer configuration file. This file is auto-generated during the "explorer" command
/// and is used to inject the runtime configuration into the explorer app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerRuntimeConfig {
    pub app_environment: String,
    pub environment_config: EnvironmentConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvironmentConfig {
    pub networks: Vec<ExplorerChainConfig>,
}

/// Explorer chain configuration file. This file is created on the chain level
/// and is used to configure the explorer for a specific chain. It serves as a building block for
/// the explorer runtime config.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerChainConfig {
    pub name: String, // L2 network chain name (the one used during the chain initialization)
    pub l2_network_name: String, // How the network is displayed in the app dropdown
    pub l2_chain_id: u64,
    pub rpc_url: String,            // L2 RPC URL
    pub api_url: String,            // L2 API URL
    pub base_token_address: String, // L2 base token address (currently always 0x800A)
    pub hostnames: Vec<String>,     // Custom domain to use when switched to this chain in the app
    pub icon: String,               // Icon to show in the explorer dropdown
    pub maintenance: bool,          // Maintenance warning
    pub published: bool, // If false, the chain will not be shown in the explorer dropdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_url: Option<String>, // Link to the portal bridge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_explorer_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_api_url: Option<String>, // L2 verification API URL
}

impl ExplorerRuntimeConfig {
    pub fn new(explorer_chain_configs: Vec<ExplorerChainConfig>) -> Self {
        Self {
            app_environment: "default".to_string(),
            environment_config: EnvironmentConfig {
                networks: explorer_chain_configs,
            },
        }
    }

    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_GENERATED_PATH)
            .join(EXPLORER_RUNTIME_CONFIG_FILE)
    }
}

impl SaveConfig for ExplorerRuntimeConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        // The block-explorer-app is served as a pre-built static app in a Docker image.
        // It uses a JavaScript file (config.js) that injects the configuration at runtime
        // by overwriting the '##runtimeConfig' property of the window object.
        // Therefore, we generate a JavaScript file instead of a JSON file.
        // This file will be mounted to the Docker image when it runs.
        let json = serde_json::to_string_pretty(&self)?;
        let config_js_content = format!("window['##runtimeConfig'] = {};", json);
        Ok(shell.write_file(path, config_js_content.as_bytes())?)
    }
}

impl ReadConfig for ExplorerRuntimeConfig {
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
        // Parse the JSON into ExplorerRuntimeConfig
        let config: ExplorerRuntimeConfig = serde_json::from_str(json_str)?;
        Ok(config)
    }
}

impl ExplorerChainConfig {
    pub fn get_config_path(ecosystem_base_path: &Path, chain_name: &str) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CHAINS_PATH)
            .join(chain_name)
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_APPS_PATH)
            .join(EXPLORER_CHAIN_CONFIG_FILE)
    }
}

impl ZkToolboxConfig for ExplorerChainConfig {}

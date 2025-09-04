use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::{
    consts::{
        EXPLORER_CONFIG_FILE, EXPLORER_JS_CONFIG_FILE, LOCAL_APPS_PATH, LOCAL_CONFIGS_PATH,
        LOCAL_GENERATED_PATH,
    },
    traits::{FileConfigTrait, ReadConfig, SaveConfig},
};

/// Explorer JSON configuration file. This file contains configuration for the explorer app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerConfig {
    pub app_environment: String,
    pub environment_config: EnvironmentConfig,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvironmentConfig {
    pub networks: Vec<ExplorerChainConfig>,
}

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
    pub prividium: bool,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ExplorerConfig {
    /// Returns the path to the explorer configuration file.
    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_APPS_PATH)
            .join(EXPLORER_CONFIG_FILE)
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
    pub fn add_chain_config(&mut self, config: &ExplorerChainConfig) {
        // Replace if config with the same network name already exists
        if let Some(index) = self
            .environment_config
            .networks
            .iter()
            .position(|c| c.name == config.name)
        {
            self.environment_config.networks[index] = config.clone();
            return;
        }
        self.environment_config.networks.push(config.clone());
    }

    /// Retains only the chains whose names are present in the given vector.
    pub fn filter(&mut self, chain_names: &[String]) {
        self.environment_config
            .networks
            .retain(|config| chain_names.contains(&config.name));
    }

    /// Hides all chains except those specified in the given vector.
    pub fn hide_except(&mut self, chain_names: &[String]) {
        for network in &mut self.environment_config.networks {
            network.published = chain_names.contains(&network.name);
        }
    }

    /// Checks if a chain with the given name exists in the configuration.
    pub fn contains(&self, chain_name: &String) -> bool {
        self.environment_config
            .networks
            .iter()
            .any(|config| &config.name == chain_name)
    }

    pub fn is_empty(&self) -> bool {
        self.environment_config.networks.is_empty()
    }

    pub fn save_as_js(&self, shell: &Shell) -> anyhow::Result<PathBuf> {
        // The block-explorer-app is served as a pre-built static app in a Docker image.
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
            .join(EXPLORER_JS_CONFIG_FILE)
    }
}

impl Default for ExplorerConfig {
    fn default() -> Self {
        ExplorerConfig {
            app_environment: "default".to_string(),
            environment_config: EnvironmentConfig {
                networks: Vec::new(),
            },
            other: serde_json::Value::Null,
        }
    }
}

impl FileConfigTrait for ExplorerConfig {}

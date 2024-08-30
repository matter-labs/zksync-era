use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;
use xshell::Shell;

use crate::{
    consts::{APPS_CONFIG_FILE, LOCAL_CHAINS_PATH, LOCAL_CONFIGS_PATH},
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig, ZkToolboxConfig},
};

pub const DEFAULT_EXPLORER_PORT: u16 = 3010;
pub const DEFAULT_PORTAL_PORT: u16 = 3030;

/// Ecosystem level configuration for the apps (portal and explorer).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppsEcosystemConfig {
    pub portal: AppEcosystemConfig,
    pub explorer: AppEcosystemConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppEcosystemConfig {
    pub http_port: u16,
    pub http_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chains_enabled: Option<Vec<String>>,
}

impl ZkToolboxConfig for AppsEcosystemConfig {}
impl FileConfigWithDefaultName for AppsEcosystemConfig {
    const FILE_NAME: &'static str = APPS_CONFIG_FILE;
}

/// Chain level configuration for the apps.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppsChainConfig {
    pub explorer: AppsChainExplorerConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppsChainExplorerConfig {
    pub database_url: Url,
    pub services: ExplorerServicesConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerServicesConfig {
    pub api_http_port: u16,
    pub data_fetcher_http_port: u16,
    pub worker_http_port: u16,
    pub batches_processing_polling_interval: Option<u64>,
}

impl ZkToolboxConfig for AppsChainConfig {}

impl AppsEcosystemConfig {
    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(APPS_CONFIG_FILE)
    }

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
}

impl Default for AppsEcosystemConfig {
    fn default() -> Self {
        AppsEcosystemConfig {
            portal: AppEcosystemConfig {
                http_port: DEFAULT_PORTAL_PORT,
                http_url: format!("http://127.0.0.1:{}", DEFAULT_PORTAL_PORT),
                chains_enabled: None,
            },
            explorer: AppEcosystemConfig {
                http_port: DEFAULT_EXPLORER_PORT,
                http_url: format!("http://127.0.0.1:{}", DEFAULT_EXPLORER_PORT),
                chains_enabled: None,
            },
        }
    }
}

impl AppsChainConfig {
    pub fn new(explorer: AppsChainExplorerConfig) -> Self {
        AppsChainConfig { explorer }
    }

    pub fn get_config_path(ecosystem_base_path: &Path, chain_name: &str) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CHAINS_PATH)
            .join(chain_name)
            .join(LOCAL_CONFIGS_PATH)
            .join(APPS_CONFIG_FILE)
    }
}

impl ExplorerServicesConfig {
    pub fn get_batches_processing_polling_interval(&self) -> u64 {
        self.batches_processing_polling_interval
            .unwrap_or(1000)
    }

    pub fn with_defaults(mut self) -> Self {
        self.batches_processing_polling_interval =
            Some(self.get_batches_processing_polling_interval());
        self
    }
}

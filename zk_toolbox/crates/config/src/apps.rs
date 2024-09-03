use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;
use xshell::Shell;

use crate::{
    consts::{
        APPS_CONFIG_FILE, DEFAULT_EXPLORER_PORT, DEFAULT_PORTAL_PORT, LOCAL_CHAINS_PATH,
        LOCAL_CONFIGS_PATH,
    },
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig, ZkToolboxConfig},
    EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL,
};

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
    pub batches_processing_polling_interval: u64,
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
            },
            explorer: AppEcosystemConfig {
                http_port: DEFAULT_EXPLORER_PORT,
                http_url: format!("http://127.0.0.1:{}", DEFAULT_EXPLORER_PORT),
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
    pub fn new(api_http_port: u16, data_fetcher_http_port: u16, worker_http_port: u16) -> Self {
        ExplorerServicesConfig {
            api_http_port,
            data_fetcher_http_port,
            worker_http_port,
            batches_processing_polling_interval: EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL,
        }
    }
}

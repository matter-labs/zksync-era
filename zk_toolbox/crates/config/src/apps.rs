use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::{
    consts::{APPS_CONFIG_FILE, LOCAL_CONFIGS_PATH},
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

impl AppsEcosystemConfig {
    pub fn default() -> Self {
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

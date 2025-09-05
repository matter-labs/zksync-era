use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xshell::Shell;

use crate::{
    consts::{APPS_CONFIG_FILE, DEFAULT_EXPLORER_PORT, DEFAULT_PORTAL_PORT, LOCAL_CONFIGS_PATH},
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig, ZkStackConfigTrait},
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
}

impl ZkStackConfigTrait for AppsEcosystemConfig {}
impl FileConfigWithDefaultName for AppsEcosystemConfig {
    const FILE_NAME: &'static str = APPS_CONFIG_FILE;
}

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
            },
            explorer: AppEcosystemConfig {
                http_port: DEFAULT_EXPLORER_PORT,
            },
        }
    }
}

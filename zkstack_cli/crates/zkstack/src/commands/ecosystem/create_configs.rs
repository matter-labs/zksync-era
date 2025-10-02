use std::path::Path;

use xshell::Shell;
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::{Erc20DeploymentConfig, InitialDeploymentConfig},
    traits::{SaveConfigWithBasePath, SaveConfigWithCommentAndBasePath},
    AppsEcosystemConfig,
};

use crate::messages::{MSG_SAVE_ERC20_CONFIG_ATTENTION, MSG_SAVE_INITIAL_CONFIG_ATTENTION};

pub fn create_initial_deployments_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<InitialDeploymentConfig> {
    let config = InitialDeploymentConfig::default();
    config.save_with_comment_and_base_path(
        shell,
        ecosystem_configs_path,
        MSG_SAVE_INITIAL_CONFIG_ATTENTION,
    )?;
    Ok(config)
}

pub fn create_erc20_deployment_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<Erc20DeploymentConfig> {
    let config = Erc20DeploymentConfig::default();
    config.save_with_comment_and_base_path(
        shell,
        ecosystem_configs_path,
        MSG_SAVE_ERC20_CONFIG_ATTENTION,
    )?;
    Ok(config)
}

pub fn create_apps_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<AppsEcosystemConfig> {
    let config = AppsEcosystemConfig::default();
    config.save_with_base_path(shell, ecosystem_configs_path)?;
    Ok(config)
}

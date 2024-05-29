use std::path::Path;

use xshell::Shell;

use crate::{
    configs::{
        forge_interface::deploy_ecosystem::input::{
            Erc20DeploymentConfig, InitialDeploymentConfig,
        },
        SaveConfigWithComment,
    },
    consts::{ERC20_DEPLOYMENT_FILE, INITIAL_DEPLOYMENT_FILE},
    messages::{MSG_SAVE_ERC20_CONFIG_ATTENTION, MSG_SAVE_INITIAL_CONFIG_ATTENTION},
};

pub fn create_initial_deployments_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<InitialDeploymentConfig> {
    let config = InitialDeploymentConfig::default();
    config.save_with_comment(
        shell,
        ecosystem_configs_path.join(INITIAL_DEPLOYMENT_FILE),
        MSG_SAVE_INITIAL_CONFIG_ATTENTION,
    )?;
    Ok(config)
}

pub fn create_erc20_deployment_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<Erc20DeploymentConfig> {
    let config = Erc20DeploymentConfig::default();
    config.save_with_comment(
        shell,
        ecosystem_configs_path.join(ERC20_DEPLOYMENT_FILE),
        MSG_SAVE_ERC20_CONFIG_ATTENTION,
    )?;
    Ok(config)
}

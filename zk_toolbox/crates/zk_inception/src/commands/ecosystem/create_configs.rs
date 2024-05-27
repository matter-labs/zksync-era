use std::path::Path;

use config::{
    forge_interface::{
        consts::{ERC20_DEPLOYMENT_FILE, INITIAL_DEPLOYMENT_FILE},
        deploy_ecosystem::input::{Erc20DeploymentConfig, InitialDeploymentConfig},
    },
    traits::SaveConfigWithComment,
};
use xshell::Shell;

pub fn create_initial_deployments_config(
    shell: &Shell,
    ecosystem_configs_path: &Path,
) -> anyhow::Result<InitialDeploymentConfig> {
    let config = InitialDeploymentConfig::default();
    config.save_with_comment(shell, ecosystem_configs_path.join(INITIAL_DEPLOYMENT_FILE), "ATTENTION: This file contains sensible placeholders. Please check them and update with the desired values.")?;
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
        "ATTENTION: This file should be filled with the desired ERC20 tokens to deploy.",
    )?;
    Ok(config)
}

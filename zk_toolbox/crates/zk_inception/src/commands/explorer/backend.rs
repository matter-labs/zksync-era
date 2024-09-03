use std::path::Path;

use anyhow::Context;
use common::{config::global_config, docker};
use config::{
    explorer_compose::*,
    traits::{ConfigWithL2RpcUrl, ReadConfig, SaveConfig},
    AppsChainConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::messages::{
    msg_explorer_chain_not_initialized, MSG_CHAIN_NOT_FOUND_ERR,
    MSG_EXPLORER_FAILED_TO_RUN_DOCKER_SERVICES_ERR,
};

pub(crate) fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let chain_name = chain_config.name.clone();
    // Read chain-level apps.yaml config
    let ecosystem_path = shell.current_dir();
    let apps_chain_config_path = AppsChainConfig::get_config_path(&ecosystem_path, &chain_name);
    if !apps_chain_config_path.exists() {
        anyhow::bail!(msg_explorer_chain_not_initialized(&chain_name));
    }
    let apps_chain_config = AppsChainConfig::read(shell, &apps_chain_config_path)?;
    // Build docker compose config with the explorer chain backend services
    let l2_rpc_url = chain_config.get_general_config()?.get_l2_rpc_url()?;
    let backend_compose_config =
        ExplorerBackendComposeConfig::new(&chain_name, l2_rpc_url, &apps_chain_config.explorer)?;
    let backend_compose_config_path =
        ExplorerBackendComposeConfig::get_config_path(&ecosystem_path, &chain_config.name);
    backend_compose_config.save(shell, backend_compose_config_path.clone())?;
    // Run docker compose
    run_backend(shell, &backend_compose_config_path)?;
    Ok(())
}

fn run_backend(shell: &Shell, explorer_compose_config_path: &Path) -> anyhow::Result<()> {
    if let Some(docker_compose_file) = explorer_compose_config_path.to_str() {
        docker::up(shell, docker_compose_file, false)
            .context(MSG_EXPLORER_FAILED_TO_RUN_DOCKER_SERVICES_ERR)?;
    } else {
        anyhow::bail!("Invalid docker compose file");
    }
    Ok(())
}

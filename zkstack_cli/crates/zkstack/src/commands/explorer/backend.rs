use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::docker;
use zkstack_cli_config::{explorer_compose::ExplorerBackendComposeConfig, ZkStackConfig};

use crate::messages::{
    msg_explorer_chain_not_initialized, MSG_EXPLORER_FAILED_TO_RUN_DOCKER_SERVICES_ERR,
};

pub(crate) fn run(shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
    let chain_name = chain_config.name.clone();
    // Read chain-level explorer backend docker compose file
    let ecosystem_path = shell.current_dir();
    let backend_config_path =
        ExplorerBackendComposeConfig::get_config_path(&ecosystem_path, &chain_config.name);
    if !backend_config_path.exists() {
        anyhow::bail!(msg_explorer_chain_not_initialized(&chain_name));
    }
    // Run docker compose
    run_backend(shell, &backend_config_path)?;
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

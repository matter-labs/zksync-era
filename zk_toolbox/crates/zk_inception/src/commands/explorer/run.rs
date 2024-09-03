use std::{collections::HashMap, path::Path};

use anyhow::Context;
use common::{config::global_config, docker, logger};
use config::{
    explorer::*,
    traits::{ReadConfig, SaveConfig},
    AppsEcosystemConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    consts::{EXPLORER_APP_DOCKER_CONFIG_PATH, EXPLORER_APP_DOCKER_IMAGE},
    messages::{
        msg_explorer_failed_to_configure_chain, msg_explorer_skipping_not_initialized_chain,
        msg_explorer_starting_on, MSG_EXPLORER_FAILED_TO_FIND_ANY_VALID,
        MSG_EXPLORER_FAILED_TO_RUN_DOCKER_ERR,
    },
};

pub(crate) fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let ecosystem_path = shell.current_dir();
    // Get ecosystem level apps.yaml config
    let apps_config = AppsEcosystemConfig::read_or_create_default(shell)?;
    // If specific_chain is provided, run only with that chain; otherwise, run with all chains
    let chains_enabled = match global_config().chain_name {
        Some(ref chain_name) => vec![chain_name.clone()],
        None => ecosystem_config.list_of_chains(),
    };

    // Read chain configs one by one
    let mut explorer_chain_configs = Vec::new();
    for chain_name in chains_enabled.iter() {
        let explorer_chain_config_path =
            ExplorerChainConfig::get_config_path(&ecosystem_path, chain_name);
        if !explorer_chain_config_path.exists() {
            logger::warn(msg_explorer_skipping_not_initialized_chain(chain_name));
            continue;
        }
        match ExplorerChainConfig::read(shell, &explorer_chain_config_path) {
            Ok(config) => explorer_chain_configs.push(config),
            Err(_) => logger::warn(msg_explorer_failed_to_configure_chain(chain_name)),
        }
    }
    if explorer_chain_configs.is_empty() {
        anyhow::bail!(MSG_EXPLORER_FAILED_TO_FIND_ANY_VALID);
    }

    // Generate and save explorer runtime config (JS)
    let runtime_config = ExplorerRuntimeConfig::new(explorer_chain_configs);
    let config_path = ExplorerRuntimeConfig::get_config_path(&ecosystem_path);
    runtime_config.save(shell, &config_path)?;

    logger::info(format!(
        "Using generated explorer config file at {}",
        config_path.display()
    ));
    logger::info(msg_explorer_starting_on(
        "127.0.0.1",
        apps_config.explorer.http_port,
    ));

    run_explorer(shell, &config_path, apps_config.explorer.http_port)?;
    Ok(())
}

fn run_explorer(shell: &Shell, config_file_path: &Path, port: u16) -> anyhow::Result<()> {
    let port_mapping = format!("{}:{}", port, port);
    let volume_mapping = format!(
        "{}:{}",
        config_file_path.display(),
        EXPLORER_APP_DOCKER_CONFIG_PATH
    );

    let mut docker_args: HashMap<String, String> = HashMap::new();
    docker_args.insert("--platform".to_string(), "linux/amd64".to_string());
    docker_args.insert("-p".to_string(), port_mapping);
    docker_args.insert("-v".to_string(), volume_mapping);
    docker_args.insert("-e".to_string(), format!("PORT={}", port));

    docker::run(shell, EXPLORER_APP_DOCKER_IMAGE, docker_args)
        .with_context(|| MSG_EXPLORER_FAILED_TO_RUN_DOCKER_ERR)?;
    Ok(())
}

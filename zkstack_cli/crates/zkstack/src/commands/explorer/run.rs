use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, docker, logger};
use zkstack_cli_config::{explorer::*, traits::SaveConfig, AppsEcosystemConfig, EcosystemConfig};

use crate::{
    consts::{
        EXPLORER_APP_DOCKER_CONFIG_PATH, EXPLORER_APP_DOCKER_IMAGE, EXPLORER_APP_DOCKER_IMAGE_TAG,
        EXPLORER_APP_PRIVIDIUM_DOCKER_IMAGE_TAG,
    },
    messages::{
        msg_explorer_running_with_config, msg_explorer_starting_on,
        MSG_EXPLORER_FAILED_TO_CREATE_CONFIG_ERR, MSG_EXPLORER_FAILED_TO_FIND_ANY_CHAIN_ERR,
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

    // Read explorer config
    let config_path = ExplorerConfig::get_config_path(&ecosystem_path);
    let mut explorer_config = ExplorerConfig::read_or_create_default(shell)
        .context(MSG_EXPLORER_FAILED_TO_CREATE_CONFIG_ERR)?;

    // Validate and update explorer config
    explorer_config.filter(&ecosystem_config.list_of_chains());
    explorer_config.hide_except(&chains_enabled);
    if explorer_config.is_empty() {
        anyhow::bail!(MSG_EXPLORER_FAILED_TO_FIND_ANY_CHAIN_ERR);
    }

    // Save explorer config
    explorer_config.save(shell, &config_path)?;

    let config_js_path = explorer_config
        .save_as_js(shell)
        .context(MSG_EXPLORER_FAILED_TO_CREATE_CONFIG_ERR)?;

    logger::info(msg_explorer_running_with_config(&config_path));
    logger::info(msg_explorer_starting_on(
        "127.0.0.1",
        apps_config.explorer.http_port,
    ));
    let name = explorer_app_name(&ecosystem_config.name);
    run_explorer(
        shell,
        &config_js_path,
        &name,
        apps_config.explorer.http_port,
        // NOTE: Prividium mode only supports one chain at the moment
        explorer_config
            .environment_config
            .networks
            .iter()
            .any(|chain| chain.prividium),
    )?;
    Ok(())
}

fn run_explorer(
    shell: &Shell,
    config_file_path: &Path,
    name: &str,
    port: u16,
    prividium: bool,
) -> anyhow::Result<()> {
    let port_mapping = format!("{}:{}", port, port);
    let volume_mapping = format!(
        "{}:{}",
        config_file_path.display(),
        EXPLORER_APP_DOCKER_CONFIG_PATH
    );

    let docker_args: Vec<String> = vec![
        "--platform".to_string(),
        "linux/amd64".to_string(),
        "--name".to_string(),
        name.to_string(),
        "-p".to_string(),
        port_mapping,
        "-v".to_string(),
        volume_mapping,
        "-e".to_string(),
        format!("PORT={}", port),
        "--rm".to_string(),
    ];

    let docker_image_tag = if prividium {
        EXPLORER_APP_PRIVIDIUM_DOCKER_IMAGE_TAG
    } else {
        EXPLORER_APP_DOCKER_IMAGE_TAG
    };
    let docker_image = docker::get_image_with_tag(EXPLORER_APP_DOCKER_IMAGE, docker_image_tag);

    docker::run(shell, &docker_image, docker_args)
        .with_context(|| MSG_EXPLORER_FAILED_TO_RUN_DOCKER_ERR)?;
    Ok(())
}

/// Generates a name for the explorer app Docker container.
/// Will be passed as `--name` argument to `docker run`.
fn explorer_app_name(ecosystem_name: &str) -> String {
    format!("{}-explorer-app", ecosystem_name)
}

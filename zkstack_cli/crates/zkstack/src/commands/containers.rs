use std::path::Path;

use anyhow::{anyhow, Context};
use xshell::Shell;
use zkstack_cli_common::{docker, logger, spinner::Spinner};
use zkstack_cli_config::{
    ZkStackConfig, ZkStackConfigTrait, DOCKER_COMPOSE_FILE, ERA_OBSERVABILITY_COMPOSE_FILE,
};

use super::args::ContainersArgs;
use crate::{
    commands::ecosystem::setup_observability,
    messages::{
        MSG_CONTAINERS_STARTED, MSG_FAILED_TO_FIND_ECOSYSTEM_ERR,
        MSG_RETRY_START_CONTAINERS_PROMPT, MSG_STARTING_CONTAINERS,
        MSG_STARTING_DOCKER_CONTAINERS_SPINNER,
    },
};

pub fn run(shell: &Shell, args: ContainersArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    let chain = ZkStackConfig::current_chain(shell).context(MSG_FAILED_TO_FIND_ECOSYSTEM_ERR)?;

    initialize_docker(shell, &chain.link_to_code())?;

    logger::info(MSG_STARTING_CONTAINERS);

    let spinner = Spinner::new(MSG_STARTING_DOCKER_CONTAINERS_SPINNER);
    if args.observability {
        setup_observability::run(shell)?;
    }

    start_containers(shell, args.observability)?;
    spinner.finish();

    logger::outro(MSG_CONTAINERS_STARTED);
    Ok(())
}

pub fn initialize_docker(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    if !shell.path_exists(DOCKER_COMPOSE_FILE) {
        copy_dockerfile(shell, link_to_code)?;
    };

    Ok(())
}

fn start_container(shell: &Shell, compose_file: &str, retry_msg: &str) -> anyhow::Result<()> {
    while let Err(err) = docker::up(shell, compose_file, true) {
        logger::error(err.to_string());
        if !zkstack_cli_common::PromptConfirm::new(retry_msg)
            .default(true)
            .ask()
        {
            return Err(err);
        }
    }
    Ok(())
}

pub fn start_containers(shell: &Shell, observability: bool) -> anyhow::Result<()> {
    start_container(
        shell,
        DOCKER_COMPOSE_FILE,
        MSG_RETRY_START_CONTAINERS_PROMPT,
    )?;

    if observability {
        start_container(
            shell,
            ERA_OBSERVABILITY_COMPOSE_FILE,
            MSG_RETRY_START_CONTAINERS_PROMPT,
        )?;
    }

    Ok(())
}

fn copy_dockerfile(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let docker_compose_file = link_to_code.join(DOCKER_COMPOSE_FILE);

    let docker_compose_text = shell.read_file(&docker_compose_file).map_err(|err| {
        anyhow!(
            "Failed to read docker compose file from {:?}: {}",
            &docker_compose_file,
            err
        )
    })?;
    let original_source = "./etc/reth/chaindata";
    let new_source = link_to_code.join(original_source);
    let new_source = new_source.to_str().unwrap();

    let data = docker_compose_text.replace(original_source, new_source);
    shell.write_file(DOCKER_COMPOSE_FILE, data)?;
    // For some reasons our docker-compose sometimes required .env file while we are investigating this behaviour
    // it's better to create file and don't make the life of customers harder
    shell.write_file(".env", "")?;
    Ok(())
}
